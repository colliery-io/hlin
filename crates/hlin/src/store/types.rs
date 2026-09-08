//! The shapes the store holds.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A breaking change a platform shipped without declaring it.
///
/// Serialised rather than typed against `hlin_manifest::Verdict` because this
/// is a record of something that happened, and a record has to keep meaning
/// what it meant after the type that produced it moves on. `changes` is the
/// diff in words for the same reason: an operator reading this a month later
/// needs to know what broke, not to be able to reconstruct a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    /// The version the platform was still declaring.
    pub declared: String,

    /// The major version the change actually needed.
    pub expected_major: u64,

    /// What broke, in the words the diff produced.
    pub changes: Vec<String>,

    /// When the shell classified it.
    pub at: DateTime<Utc>,

    /// How many times this platform has done this, across restarts.
    ///
    /// A platform that violates once has an accident; one that violates every
    /// week has a process problem, and those want different conversations.
    pub seen: i32,
}

/// What a platform was last seen to be serving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformSnapshot {
    /// The platform, as shell configuration names it.
    pub platform_id: String,

    /// The manifest exactly as it was fetched, so a later version of the shell
    /// can re-read it with fields this one ignored.
    pub manifest: Value,

    /// The fingerprint the shell computed over its contract content.
    pub contract_hash: String,

    /// The version the platform declared.
    pub contract_version: String,

    /// When this was last seen.
    pub observed_at: DateTime<Utc>,

    /// How many consecutive polls have returned this same contract hash.
    ///
    /// The registry waits for this to cross a threshold before classifying a
    /// change, so a deployment flapping between two revisions does not raise a
    /// violation on every flip.
    pub consecutive_observations: i32,

    /// The last time this platform shipped a breaking change without declaring
    /// it, if it ever has.
    ///
    /// Persisted because the detection was previously a single `tracing::error!`
    /// at the moment of classification, and nothing else. HLIN-A-0002 calls a
    /// violation "an operator signal", and an operator who was not tailing the
    /// log when it fired could not answer the one question the whole mechanism
    /// exists to answer: has this platform ever done this?
    ///
    /// Kept on the snapshot rather than in a table of its own because one
    /// question is being answered, not a history: what an operator needs is
    /// "has it, and when", and a platform that violates repeatedly is one
    /// problem rather than many.
    #[serde(default)]
    pub last_violation: Option<Violation>,
}

/// Who may see a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Its owner, and anyone holding its link.
    Personal,
    /// Listed in the shell-wide gallery, visible to every authenticated
    /// principal.
    Published,
}

impl Visibility {
    /// The string this is stored as.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Published => "published",
        }
    }

    /// Read one back from storage.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "personal" => Some(Self::Personal),
            "published" => Some(Self::Published),
            _ => None,
        }
    }
}

/// A surface someone composed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// Its identifier, and the thing a shared link carries.
    pub id: Uuid,

    /// The principal who created it. Exactly one owner (decision HLIN-A-0007).
    pub owner: String,

    /// What they called it.
    pub title: String,

    /// Who may see it.
    pub visibility: Visibility,

    /// Where this was copied from, if it was.
    pub forked_from: Option<Uuid>,

    /// The time range the author meant, so a shared layout opens the way they
    /// left it.
    pub time_range: Option<Value>,

    /// The panels on it, in the order they were stored.
    pub panels: Vec<PanelInstance>,

    /// When it was made.
    pub created_at: DateTime<Utc>,

    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

impl Layout {
    /// Whether this layout is listed in the gallery.
    pub fn is_published(&self) -> bool {
        self.visibility == Visibility::Published
    }

    /// Whether a principal may change this layout.
    ///
    /// Only its owner. Everyone else reads it and forks to edit, which is what
    /// keeps one source of truth per layout.
    pub fn is_editable_by(&self, principal: &str) -> bool {
        self.owner == principal
    }

    /// Whether a principal may open this layout.
    ///
    /// A published layout is open to anyone; a personal one is open to its
    /// owner and to anyone who was given its link, which the shell cannot
    /// distinguish from anyone else who knows the identifier. Link sharing is
    /// exactly that: knowing the link is the permission.
    pub fn is_visible_to(&self, principal: &str) -> bool {
        self.is_published() || self.owner == principal
    }
}

/// What is needed to create a layout.
#[derive(Debug, Clone, PartialEq)]
pub struct NewLayout {
    /// Who will own it.
    pub owner: String,
    /// What to call it.
    pub title: String,
    /// Who may see it. New layouts are personal unless said otherwise.
    pub visibility: Visibility,
    /// Where it was copied from, if it was.
    pub forked_from: Option<Uuid>,
    /// The time range it opens at.
    pub time_range: Option<Value>,
    /// The panels to put on it.
    pub panels: Vec<PanelInstance>,
}

impl NewLayout {
    /// An empty personal layout.
    pub fn personal(owner: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            title: title.into(),
            visibility: Visibility::Personal,
            forked_from: None,
            time_range: None,
            panels: Vec::new(),
        }
    }
}

/// One panel on one surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelInstance {
    /// Its identifier within the surface. The stream names this in every frame.
    pub id: Uuid,

    /// The platform the panel comes from.
    pub platform_id: String,

    /// The panel's key in that platform's manifest.
    pub panel_key: String,

    /// The kind the viewer chose, where it differs from the manifest default.
    ///
    /// `None` means "whatever the manifest says", so a platform changing its
    /// default reaches everyone who did not override it.
    pub kind_override: Option<String>,

    /// The title the viewer chose, where it differs from the manifest default.
    pub title_override: Option<String>,

    /// Values for this instance's own parameters.
    pub selections: Value,

    /// Composition-level customisations, such as thresholds.
    pub customizations: Value,

    /// Where it sits on the surface.
    pub position: Value,

    /// When it was added.
    pub created_at: DateTime<Utc>,
}

impl PanelInstance {
    /// A panel instance referencing `platform_id/panel_key`, with defaults
    /// everywhere the viewer has not chosen otherwise.
    pub fn new(
        platform_id: impl Into<String>,
        panel_key: impl Into<String>,
        position: Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            platform_id: platform_id.into(),
            panel_key: panel_key.into(),
            kind_override: None,
            title_override: None,
            selections: Value::Object(Default::default()),
            customizations: Value::Object(Default::default()),
            position,
            created_at: Utc::now(),
        }
    }

    /// The manifest reference this instance points at, as `platform_id/key`.
    pub fn reference(&self) -> String {
        format!("{}/{}", self.platform_id, self.panel_key)
    }
}

/// A sign-in the shell issued, as it is stored.
///
/// The cookie's value is never here: `id` is its SHA-256. See the migration for
/// why, and for why this is a row at all rather than a signed cookie.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The SHA-256 of the cookie value, hex.
    pub id: String,

    /// Who the identity provider said this is.
    pub subject: String,

    /// Their display name, where the provider offered one.
    pub name: Option<String>,

    /// The groups claim, as read at sign-in.
    pub groups: Vec<String>,

    /// When they signed in.
    pub created_at: DateTime<Utc>,

    /// When this stops being them.
    pub expires_at: DateTime<Utc>,
}

impl Session {
    /// Whether this session is still current at a given moment.
    ///
    /// Checked on read as well as swept in the background: a sweeper that has
    /// not run yet must never be the reason an expired session still works.
    pub fn is_current_at(&self, moment: DateTime<Utc>) -> bool {
        moment < self.expires_at
    }
}

/// A sign-in that has been started and not yet finished.
///
/// Everything the callback needs to prove that the browser redeeming a code is
/// the browser that asked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingLogin {
    /// The `state` parameter, as sent to the provider.
    pub state: String,

    /// The nonce, to be found in the id token.
    pub nonce: String,

    /// The PKCE verifier whose challenge was sent.
    pub code_verifier: String,

    /// Where to send the person once they are known. A path on this shell.
    pub redirect_to: String,

    /// When the sign-in was started.
    pub created_at: DateTime<Utc>,

    /// After which this sign-in is abandoned rather than merely unfinished.
    pub expires_at: DateTime<Utc>,
}
