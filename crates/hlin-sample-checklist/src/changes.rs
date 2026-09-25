//! Telling anyone who asked that a list changed.
//!
//! The checklist's side of [[HLIN-S-0006]]. Every change a person makes is
//! announced on the platform's event stream as the `items` panel with that
//! list selected, so a shell refetches the instances showing that list and
//! leaves the others alone. The event carries no data, as the specification
//! requires: what a list holds depends on who is asking, and only a fetch made
//! as them can say.
//!
//! This is also how two shells that know nothing of each other stay in step.
//! The shell relays `changed` between modules on its own page, but a second
//! shell pointed at the same platform only hears about a write through here.

use tokio::sync::broadcast;

/// The one panel this platform reports on.
pub const ITEMS: &str = "items";

/// The selection parameter that names a list.
pub const LIST_PARAM: &str = "list";

/// How many announcements a slow subscriber may fall behind by.
///
/// Small for the reason `hlin-sample-platform` gives: a missed notice costs one
/// relaxed poll of staleness, and a subscriber that far behind is told that
/// everything changed instead.
const BACKLOG: usize = 64;

/// Who to tell when a list changes.
#[derive(Debug)]
pub struct Changes {
    told: broadcast::Sender<String>,
}

impl Default for Changes {
    fn default() -> Self {
        Self::new()
    }
}

impl Changes {
    /// Nobody listening yet.
    pub fn new() -> Self {
        let (told, _) = broadcast::channel(BACKLOG);
        Self { told }
    }

    /// Listen for lists that change, by list id.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.told.subscribe()
    }

    /// Whether anybody is listening, for the tests.
    pub fn listeners(&self) -> usize {
        self.told.receiver_count()
    }

    /// Say that a list changed.
    ///
    /// Called only after the write has landed, for the reason the sample
    /// platform's `changes.rs` spells out: announce first, and a fast shell
    /// refetches the old list and then waits out the relaxed interval.
    pub fn list_changed(&self, list: &str) {
        // Fails only when nobody is subscribed, which is not worth a word.
        let _ = self.told.send(list.to_string());
    }
}

/// The `data` of a `changed` event.
///
/// With a list, the event narrows to instances showing that list. Without
/// one, it reaches every instance of the panel, which is what a subscriber
/// that fell behind is sent: it does not know which lists it missed.
pub fn event_data(list: Option<&str>) -> serde_json::Value {
    match list {
        Some(list) => serde_json::json!({
            "panel": ITEMS,
            "selections": { LIST_PARAM: [list] },
        }),
        None => serde_json::json!({ "panel": ITEMS }),
    }
}
