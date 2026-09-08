//! The state a panel is in, and what a rendering must account for.
//!
//! Every panel is in exactly one state at all times. The aggregator decides
//! which (decision HLIN-A-0001) and the renderer draws what it is told; the
//! types here are the shared vocabulary between them.
//!
//! The set is closed on purpose. "Rendering is total over its inputs" is only
//! a real guarantee if the states are enumerable, and this enum is what makes
//! the totality test in this crate possible.

use std::fmt;

/// Where a panel is right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PanelState {
    /// Nothing has arrived yet for the request in flight.
    Loading,
    /// The latest envelope is fresh.
    Ready,
    /// The last envelope is older than the staleness window, or the browser
    /// has lost its stream to the shell. Data is still shown, visibly aged.
    Stale,
    /// Nothing usable can be shown, for a reason worth distinguishing.
    Unavailable(Cause),
}

impl PanelState {
    /// Every state a panel can be in.
    ///
    /// The totality test walks this, so a state added here without a rendering
    /// fails the build rather than escaping into a layout.
    pub fn all() -> Vec<Self> {
        let mut states = vec![Self::Loading, Self::Ready, Self::Stale];
        states.extend(Cause::all().into_iter().map(Self::Unavailable));
        states
    }

    /// Whether this state can show the panel's data.
    ///
    /// `Loading` has none yet, and the causes that mean "what arrived cannot be
    /// trusted" refuse to show what they have. `Stale` and `Unreachable` still
    /// show the last good envelope, because out-of-date data with its age on
    /// screen beats an empty box during an incident.
    pub fn shows_data(&self) -> bool {
        match self {
            Self::Ready | Self::Stale => true,
            Self::Loading => false,
            Self::Unavailable(cause) => cause.keeps_last_envelope(),
        }
    }
}

impl fmt::Display for PanelState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loading => formatter.write_str("loading"),
            Self::Ready => formatter.write_str("ready"),
            Self::Stale => formatter.write_str("stale"),
            Self::Unavailable(cause) => write!(formatter, "unavailable ({cause})"),
        }
    }
}

/// Why a panel has nothing to show.
///
/// Four of these come from the vision; `Forbidden` was added when
/// authentication was hoisted to the shell and platforms took over
/// authorisation at fetch time (decision HLIN-A-0004). They are kept apart
/// because the person looking at the panel needs different things from each:
/// wait, tell someone, pick the successor, or ask for access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cause {
    /// The platform did not answer, or the shell itself has gone away.
    Unreachable,
    /// What arrived could not be used: a bad manifest declaration, an envelope
    /// that does not match what was promised, or one over its limits.
    Malformed,
    /// The panel or its platform is no longer in the registry.
    Unknown,
    /// The panel is past the sunset its platform declared.
    Deprecated,
    /// The platform refused this viewer.
    Forbidden,
}

impl Cause {
    /// Every reason a panel can be unavailable.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Unreachable,
            Self::Malformed,
            Self::Unknown,
            Self::Deprecated,
            Self::Forbidden,
        ]
    }

    /// Whether the last good envelope may still be shown, dimmed.
    ///
    /// Only `Unreachable` keeps it. The rest mean the data is wrong, gone, or
    /// not this viewer's to see, and showing it would mislead.
    pub fn keeps_last_envelope(&self) -> bool {
        matches!(self, Self::Unreachable)
    }

    /// Whether the panel should offer the successor its platform named.
    pub fn offers_successor(&self) -> bool {
        matches!(self, Self::Deprecated | Self::Unknown)
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self {
            Self::Unreachable => "unreachable",
            Self::Malformed => "malformed",
            Self::Unknown => "unknown",
            Self::Deprecated => "deprecated",
            Self::Forbidden => "forbidden",
        };
        formatter.write_str(word)
    }
}
