//! What the shell remembers.
//!
//! The shell is stateful, and deliberately so. Two things live here, for two
//! different reasons.
//!
//! **Contract memory.** The registry compares each manifest against the last
//! one it saw. Without somewhere durable to keep that, a restart forgets what
//! every platform last promised, and an undeclared breaking change shipped
//! across the restart window goes undetected. The whole runtime-enforcement
//! model in decision HLIN-A-0002 rests on this surviving a redeploy.
//!
//! **Layouts.** The surfaces people compose, which is the thing the system
//! exists to let them do.
//!
//! Runtime caches are not here. Per-principal option lists, in-flight
//! deduplication and staleness timers are process-local and rebuilt on start.
//! Persisting them would buy nothing and could serve someone data from before a
//! restart with nothing on screen saying so.
//!
//! The backend is Postgres (decision HLIN-A-0006). The trait below exists for
//! testability, not portability: [`memory::MemoryStore`] is a test double, and
//! there is no second supported backend.

use async_trait::async_trait;

pub mod memory;
pub mod postgres;
pub mod types;

pub use memory::MemoryStore;
pub use postgres::PostgresStore;
pub use types::{
    Layout, NewLayout, PanelInstance, PendingLogin, PlatformSnapshot, Session, Violation,
    Visibility,
};

/// Why a store operation could not be completed.
///
/// Deliberately small. A store failure is an operational problem, not a domain
/// outcome: everything the shell treats as an ordinary answer, such as a
/// platform not being known yet, is `Ok(None)` rather than an error.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The layout, panel instance or platform does not exist.
    #[error("no such {entity}: {id}")]
    NotFound {
        /// What kind of thing was being looked for.
        entity: &'static str,
        /// The identifier that found nothing.
        id: String,
    },

    /// The store itself failed.
    #[error("store unavailable: {0}")]
    Unavailable(String),

    /// Stored data could not be read back into the shape the shell expects.
    ///
    /// This means a migration and the code disagree, which is a deployment
    /// problem rather than anything a user did.
    #[error("stored data is not readable: {0}")]
    Corrupt(String),
}

/// Result alias for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Everything the shell needs to remember.
///
/// One trait rather than several, because the two halves are small and a shell
/// holding one handle is simpler than one holding three. If either half grows
/// enough to make this unwieldy, splitting it is a refactor with no consequence
/// outside this module.
#[async_trait]
pub trait Store: Send + Sync {
    // -- Contract memory --------------------------------------------------

    /// The last manifest seen for a platform, if the shell has ever seen one.
    ///
    /// `Ok(None)` for a platform the shell is meeting for the first time. That
    /// is an ordinary situation on a first poll after a deploy, not an error.
    async fn platform_snapshot(&self, platform_id: &str) -> Result<Option<PlatformSnapshot>>;

    /// Every platform the shell has a snapshot for.
    async fn platform_snapshots(&self) -> Result<Vec<PlatformSnapshot>>;

    /// Record what a platform is currently serving.
    ///
    /// When the contract hash matches what is already stored, this increments
    /// the consecutive-observation count rather than resetting it; when it
    /// differs, the count starts again at one. That count is what the registry
    /// debounces on, so a blue/green rollout flapping between two revisions
    /// does not raise a violation per flip (decision HLIN-A-0002).
    async fn observe_platform(&self, snapshot: PlatformSnapshot) -> Result<PlatformSnapshot>;

    /// Record that a platform shipped a breaking change without declaring it.
    ///
    /// Separate from `observe_platform` because the two answer different
    /// questions and happen at different moments: an observation is what a
    /// platform is serving now, and a violation is a judgement made after
    /// enough observations agree. Returns the record as stored, with its count
    /// incremented.
    async fn record_violation(
        &self,
        platform_id: &str,
        declared: &str,
        expected_major: u64,
        changes: Vec<String>,
    ) -> Result<Violation>;

    /// Forget a platform, because configuration no longer lists it.
    ///
    /// Returns whether there was anything to forget.
    async fn forget_platform(&self, platform_id: &str) -> Result<bool>;

    // -- Layouts ----------------------------------------------------------

    /// One layout and its panels.
    async fn layout(&self, id: uuid::Uuid) -> Result<Option<Layout>>;

    /// Every layout a principal owns, most recently changed first.
    async fn layouts_owned_by(&self, owner: &str) -> Result<Vec<Layout>>;

    /// The gallery: every published layout, most recently changed first.
    ///
    /// Visible to every authenticated principal, whoever owns them
    /// (decision HLIN-A-0007).
    async fn published_layouts(&self) -> Result<Vec<Layout>>;

    /// Create a layout.
    async fn create_layout(&self, layout: NewLayout) -> Result<Layout>;

    /// Replace a layout's panels and settings.
    ///
    /// Whole-layout replacement rather than per-panel edits, because a layout
    /// is edited as one thing: dragging a panel changes several positions at
    /// once, and there is no useful notion of half of that having happened.
    async fn update_layout(&self, layout: &Layout) -> Result<()>;

    /// Delete a layout and its panels.
    ///
    /// Forks of it survive, with their provenance pointer cleared.
    async fn delete_layout(&self, id: uuid::Uuid) -> Result<bool>;

    /// Copy a layout for someone who wants to change one they do not own.
    ///
    /// This is the whole of the sharing model's write path: a viewer edits, and
    /// gets their own copy with a record of where it came from
    /// (decision HLIN-A-0007).
    async fn fork_layout(&self, id: uuid::Uuid, new_owner: &str) -> Result<Layout>;

    // -- Sessions ---------------------------------------------------------
    //
    // Only the `oidc` authenticator uses these: `dev` invents a principal and
    // `trusted-header` is told one by a proxy, and neither has anything to
    // remember between requests. They are on the one trait anyway rather than
    // behind a second handle, for the reason the trait's own documentation
    // gives — one handle is simpler than two, and this half is small.

    /// Remember a sign-in that has been started.
    async fn begin_login(&self, pending: types::PendingLogin) -> Result<()>;

    /// Take back a started sign-in, removing it.
    ///
    /// Removing is the point rather than a tidy-up: `state` is a one-time
    /// value, so a second callback carrying the same one must find nothing.
    /// Returns `Ok(None)` for a state that was never issued, was already
    /// redeemed, or has expired — three situations a caller cannot usefully
    /// tell apart and none of which is an error.
    async fn claim_login(&self, state: &str) -> Result<Option<types::PendingLogin>>;

    /// Record a session the shell has issued.
    async fn create_session(&self, session: types::Session) -> Result<()>;

    /// The session with this id, if it exists and has not expired.
    ///
    /// Expiry is applied here as well as by the sweeper. A sweeper that has not
    /// run yet must never be the reason an expired session still works.
    async fn session(&self, id: &str) -> Result<Option<types::Session>>;

    /// End one session. Returns whether there was one to end.
    async fn end_session(&self, id: &str) -> Result<bool>;

    /// Delete every expired session and abandoned sign-in.
    ///
    /// Returns how many rows went. Called on a timer; the table would otherwise
    /// grow with every sign-in ever made and never shrink.
    async fn sweep_expired(&self) -> Result<u64>;
}
