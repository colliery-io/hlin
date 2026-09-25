#![warn(missing_docs)]

//! The wire types Hlin's shell and browser share.
//!
//! Its own crate for one reason: these types are read by the shell *and* by
//! the browser, and the shell depends on sqlx, tokio and axum, none of which
//! build for `wasm32`. Keeping the wire shapes here means both sides read one
//! definition rather than two that drift.
//!
//! Two protocols live here. The stream, below, is what the shell pushes to a
//! browser watching a surface. [`layout`] is what the two say to each other
//! about the surface itself: the layout being composed, and the catalogue of
//! panels it can be composed from. [`streamed`] is how the shell hands its
//! page a module's streamed response, with the reason it ended.
//!
//! What travels from the shell to a browser.
//!
//! The second versioned contract (decision HLIN-A-0001), and the reason a
//! panel's state can be decided where the network facts are while still
//! reaching the thing that draws it. State travels *beside* the envelope, not
//! inside it, which is what keeps envelopes pure data.
//!
//! Additive, like the manifest: unknown fields are ignored and an unknown
//! frame type is skipped, so a browser and a shell of different vintages
//! interoperate.

pub mod layout;
pub mod streamed;

use chrono::{DateTime, Utc};
use hlin_manifest::Envelope;
use hlin_view::{Cause, PanelState};
use serde::{Deserialize, Serialize};

/// The version of this protocol the shell speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// A panel's state, on the wire.
///
/// Flattened into `state` plus `cause` rather than nested, because a browser
/// reading `"state": "unavailable"` and then `"cause"` is doing what the
/// specification's table says, and a nested shape would invite an unknown
/// cause to be read as an unknown state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireState {
    /// Nothing has arrived yet.
    Loading,
    /// Fresh.
    Ready,
    /// Older than the staleness window, or the stream is gone.
    Stale,
    /// Nothing usable, for the reason in `cause`.
    Unavailable,
}

/// Why a panel is unavailable, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireCause {
    /// The platform, or the shell itself, is not answering.
    Unreachable,
    /// What arrived could not be used.
    Malformed,
    /// The panel or platform is gone from the registry.
    Unknown,
    /// Past the panel's sunset.
    Deprecated,
    /// The platform refused this viewer.
    Forbidden,
}

impl From<Cause> for WireCause {
    fn from(cause: Cause) -> Self {
        match cause {
            Cause::Unreachable => Self::Unreachable,
            Cause::Malformed => Self::Malformed,
            Cause::Unknown => Self::Unknown,
            Cause::Deprecated => Self::Deprecated,
            Cause::Forbidden => Self::Forbidden,
        }
    }
}

impl From<WireCause> for Cause {
    fn from(cause: WireCause) -> Self {
        match cause {
            WireCause::Unreachable => Self::Unreachable,
            WireCause::Malformed => Self::Malformed,
            WireCause::Unknown => Self::Unknown,
            WireCause::Deprecated => Self::Deprecated,
            WireCause::Forbidden => Self::Forbidden,
        }
    }
}

/// The state of one panel instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelFrame {
    /// The protocol this frame is written in.
    pub protocol_version: u32,

    /// Which panel on the surface.
    pub instance: String,

    /// Which round of parameters this answers. A browser drops a frame from a
    /// generation it has moved past.
    pub generation: u64,

    /// Where the panel is.
    pub state: WireState,

    /// Required when `state` is `unavailable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<WireCause>,

    /// When the data was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// How old it is, so the browser needs no clock of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<i64>,

    /// The document, present only where the state shows data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,

    /// One line for the viewer, written by the shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// What replaces this panel, where its platform said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor: Option<String>,
}

impl PanelFrame {
    /// The state this frame carries, as the renderer's type.
    pub fn panel_state(&self) -> PanelState {
        match self.state {
            WireState::Loading => PanelState::Loading,
            WireState::Ready => PanelState::Ready,
            WireState::Stale => PanelState::Stale,
            WireState::Unavailable => {
                PanelState::Unavailable(self.cause.map(Cause::from).unwrap_or(Cause::Malformed))
            }
        }
    }

    /// A frame for a panel in this state.
    pub fn new(instance: impl Into<String>, generation: u64, state: PanelState) -> Self {
        let (state, cause) = match state {
            PanelState::Loading => (WireState::Loading, None),
            PanelState::Ready => (WireState::Ready, None),
            PanelState::Stale => (WireState::Stale, None),
            PanelState::Unavailable(cause) => (WireState::Unavailable, Some(cause.into())),
        };

        Self {
            protocol_version: PROTOCOL_VERSION,
            instance: instance.into(),
            generation,
            state,
            cause,
            as_of: None,
            age_seconds: None,
            envelope: None,
            detail: None,
            successor: None,
        }
    }
}

/// Something about the surface as a whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceFrame {
    /// The protocol this frame is written in.
    pub protocol_version: u32,
    /// The generation now in force.
    pub generation: u64,
    /// That the shell received a parameter change, before any panel has
    /// answered it.
    pub acknowledged: bool,
}

impl SurfaceFrame {
    /// An acknowledgement of a generation.
    pub fn acknowledging(generation: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            generation,
            acknowledged: true,
        }
    }
}

/// Where a `changed` frame's news came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOrigin {
    /// The platform's own event stream (HLIN-S-0006).
    Platform,
    /// One of the platform's modules, after a write, on any surface this shell
    /// serves (HLIN-S-0007).
    Module,
}

/// Something on a platform changed, for that platform's modules on this
/// surface.
///
/// The browser turns it into the module bridge's `changed` (HLIN-S-0007) for
/// every module of the platform it has mounted. Sent only to a surface whose
/// layout holds one of that platform's modules: shell-drawn panels need no
/// frame, because the shell refetches them itself and their new data arrives
/// as `panel` frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedFrame {
    /// The protocol this frame is written in.
    pub protocol_version: u32,
    /// The platform that changed.
    pub platform: String,
    /// The panel key the change concerns, as that platform's manifest names it.
    pub panel: String,
    /// The selections it was made under; empty means every instance.
    #[serde(default)]
    pub selections: std::collections::BTreeMap<String, Vec<String>>,
    /// Who said so.
    pub from: ChangeOrigin,
    /// For a module's change, the page that relayed it, which has already told
    /// its own modules and skips this frame. An opaque id the page chose; it
    /// names a page load and nothing else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// Anything the shell sends.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// One panel changed.
    Panel(Box<PanelFrame>),
    /// The surface changed.
    Surface(SurfaceFrame),
    /// Something on a platform whose modules are on this surface changed.
    Changed(ChangedFrame),
}

impl Frame {
    /// The server-sent event name, so a browser can ignore a type it does not
    /// know.
    pub fn event(&self) -> &'static str {
        match self {
            Self::Panel(_) => "panel",
            Self::Surface(_) => "surface",
            Self::Changed(_) => "changed",
        }
    }

    /// The frame as JSON.
    pub fn to_json(&self) -> String {
        match self {
            Self::Panel(frame) => serde_json::to_string(frame),
            Self::Surface(frame) => serde_json::to_string(frame),
            Self::Changed(frame) => serde_json::to_string(frame),
        }
        .unwrap_or_else(|_| "{}".to_string())
    }
}

/// What the page sends when one of its modules says it changed something
/// (`POST /api/stream/{surface}/changed`).
///
/// The platform is the frame's, which the page knows from its registry; a
/// module never names one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedRequest {
    /// The platform whose module wrote.
    pub platform: String,
    /// The panel the change concerns.
    pub panel: String,
    /// The selections it was made under.
    #[serde(default)]
    pub selections: std::collections::BTreeMap<String, Vec<String>>,
    /// The page relaying it, echoed in the frames so it can skip its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// What a browser sends when a control moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamsRequest {
    /// The protocol the browser is speaking.
    #[serde(default = "default_protocol")]
    pub protocol_version: u32,

    /// The generation these parameters belong to. A generation below the one
    /// in force is ignored, so an out-of-order message cannot rewind a
    /// surface.
    pub generation: u64,

    /// The surface's time range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,

    /// Per-instance parameter values, by instance id then by parameter id.
    #[serde(default)]
    pub selections:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<String>>>,
}

fn default_protocol() -> u32 {
    PROTOCOL_VERSION
}

/// The window every panel that asked for one is shown.
///
/// Absolute instants: the shell resolves anything relative before encoding, so
/// a platform never sees `now-1h` (HLIN-S-0002 REQ-2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start.
    pub from: DateTime<Utc>,
    /// End.
    pub to: DateTime<Utc>,
}

impl TimeRange {
    /// The resolution hint for this window at this width, in seconds.
    ///
    /// A hint, not an instruction: a platform may ignore it, and one serving
    /// `series.v1` should downsample to about this.
    pub fn step_seconds(&self, panel_width_pixels: u32) -> i64 {
        let span = (self.to - self.from).num_seconds().max(1);
        let points = panel_width_pixels.clamp(120, 2000) as i64;
        (span / points).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_frame_is_its_own_event_and_says_where_it_came_from() {
        let frame = Frame::Changed(ChangedFrame {
            protocol_version: PROTOCOL_VERSION,
            platform: "checklist".into(),
            panel: "items".into(),
            selections: [("list".to_string(), vec!["team".to_string()])].into(),
            from: ChangeOrigin::Module,
            page: Some("p-1".into()),
        });

        // Its own event name, so a browser from before it existed never sees
        // it: an `EventSource` only delivers the names it listens for.
        assert_eq!(frame.event(), "changed");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&frame.to_json()).unwrap(),
            serde_json::json!({
                "protocol_version": 1, "platform": "checklist", "panel": "items",
                "selections": { "list": ["team"] }, "from": "module", "page": "p-1"
            })
        );
    }

    #[test]
    fn a_platform_change_names_no_page_and_reads_without_selections() {
        let read: ChangedFrame = serde_json::from_value(serde_json::json!({
            "protocol_version": 1, "platform": "orebank", "panel": "batches",
            "from": "platform", "someday": true
        }))
        .unwrap();
        assert!(read.selections.is_empty());
        assert_eq!(read.from, ChangeOrigin::Platform);
        assert!(!Frame::Changed(read).to_json().contains("page"));
    }
}
