//! What a component can say back.
//!
//! Everything else in this crate flows one way: the shell knows something about
//! a panel, and a design pack draws it. This is the return path, and it is
//! deliberately much narrower than the one going out.
//!
//! # Why a closed vocabulary again
//!
//! A component could be handed a general callback and allowed to say anything.
//! It would be less code here and it would move the problem: the shell would
//! have to interpret whatever arrived, and "whatever arrived" is not something
//! it can be tested against or degrade from.
//!
//! So an [`Intent`] is one of a small set of things, and every one of them maps
//! onto something the shell already does and already has tested. A pack asking
//! for a selection is asking for the same change the parameter control makes; a
//! pack asking for a range is asking for what the time picker asks for. Nothing
//! here gives a design system a capability the chrome does not have, which is
//! the property that makes forwarding it safe.
//!
//! # What deliberately is not here
//!
//! Anything a component can do for itself. A node that expands, a column that
//! sorts, a tooltip: those are local to the component, the pack holds that
//! state, and involving the shell would be a round trip for nothing. This is
//! for what a component cannot do alone, which is exactly the set of things
//! that change what the shell asks a platform for.

use hlin_manifest::envelope::Choice;

use std::fmt;
use std::sync::Arc;

/// Something a person did to a component, in terms the shell can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Set a parameter on the panel this came from.
    ///
    /// The same change the panel's own control makes, so it goes to the same
    /// place: the instance's selections, which are stored with the layout and
    /// sent to the platform as query parameters. An empty `values` clears the
    /// choice and gives the platform its default back.
    ///
    /// `param` is the control's id — the key a platform declared, such as
    /// `cluster` — and not the vocabulary word `select`.
    Select {
        /// Which parameter.
        param: String,
        /// What it should now be.
        values: Vec<String>,
    },

    /// Set the time range for the whole surface.
    ///
    /// Surface-wide rather than panel-local because that is what a time range
    /// is here: one picker drives every panel, so a chart brushed to an hour
    /// moves everything beside it. Milliseconds since the epoch, which is what
    /// `series.v1` points already carry, so a component reading a brush off its
    /// own axis has nothing to convert.
    Range {
        /// Start, inclusive.
        from_millis: i64,
        /// End, inclusive.
        to_millis: i64,
    },
}

/// Where a component sends what a person did.
///
/// Cheap to clone and safe to keep: a component captures one in an event
/// handler and calls it whenever. Cloning is an `Arc` bump, which matters
/// because a pack drawing a table of a hundred rows may want one per row.
///
/// An `Emit` that goes [`nowhere`](Emit::nowhere) is not an error case and not
/// a degraded mode. It is what a pack gets when it is drawn outside a live
/// surface — a test, a snapshot, a static render — and it means a component can
/// wire its handlers unconditionally instead of having two versions of itself.
#[derive(Clone, Default)]
pub struct Emit(Option<Arc<dyn Fn(Intent) + Send + Sync>>);

impl Emit {
    /// An emitter that discards everything.
    pub fn nowhere() -> Self {
        Self(None)
    }

    /// An emitter that calls this.
    ///
    /// `Send + Sync` because Leptos requires it of anything captured in a
    /// reactive closure, which is where every one of these ends up.
    pub fn to(sink: impl Fn(Intent) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(sink)))
    }

    /// Say what a person did.
    pub fn send(&self, intent: Intent) {
        if let Some(sink) = &self.0 {
            sink(intent);
        }
    }

    /// Whether anything is listening.
    ///
    /// Worth checking before drawing a control that would do nothing: an
    /// interactive-looking thing that is inert is worse than a static one.
    pub fn is_live(&self) -> bool {
        self.0.is_some()
    }
}

impl fmt::Debug for Emit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Emit")
            .field(&if self.is_live() { "live" } else { "nowhere" })
            .finish()
    }
}

/// A parameter this panel responds to, and what it is currently set to.
///
/// Carried so a component can draw its own filter rather than leaving it to the
/// chrome, and carrying both halves of what that needs: what the platform will
/// accept, and what is chosen now.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// The vocabulary word, such as `select`.
    pub param: String,
    /// The key this is stored and sent under, such as `cluster`.
    pub id: String,
    /// What to call it on screen.
    pub label: String,
    /// What it is currently set to, empty where the platform's default applies.
    pub chosen: Vec<String>,
    /// What the platform said it will accept.
    ///
    /// Empty where the platform declared no options endpoint, where it has not
    /// answered yet, or where it answered badly. A component seeing an empty
    /// list should take a typed value rather than refuse to draw: the platform
    /// is still free to accept something it never listed.
    pub choices: Vec<Choice>,
}

impl Control {
    /// The single chosen value, for the common case of a control that takes
    /// one.
    pub fn value(&self) -> Option<&str> {
        self.chosen.first().map(String::as_str)
    }
}
