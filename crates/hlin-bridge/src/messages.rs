//! Every message, in both directions, field for field as specification
//! HLIN-S-0007 writes them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};

use crate::Message;

/// Parameter selections: a parameter id to the values selected for it.
pub type Selections = BTreeMap<String, Vec<String>>;

// ---------------------------------------------------------------------------
// Shell to module
// ---------------------------------------------------------------------------

/// A message from the shell's page to a module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "kebab-case")]
pub enum ShellMessage {
    /// Once, after the frame loads: everything a module needs to draw.
    Init(Init),
    /// The time range or a parameter changed.
    Context(Context),
    /// The scheme or the tokens changed.
    Theme(Theme),
    /// The module's platform, or another of its modules, changed something.
    Changed(ShellChanged),
    /// The panel scrolled out of view or back, or the tab was hidden.
    Visibility(Visibility),
    /// The answer to a `fetch`.
    Response(Response),
    /// Are you there? Echo it.
    Heartbeat(Heartbeat),
    /// A part of a streamed response body.
    Chunk(Chunk),
    /// The end of a streamed response body.
    End(End),
    /// The frame is about to be unmounted; answer with `state` to keep
    /// anything.
    Suspend(Suspend),
}

impl Message for ShellMessage {
    const TYPES: &'static [&'static str] = &[
        "init",
        "context",
        "theme",
        "changed",
        "visibility",
        "response",
        "heartbeat",
        "chunk",
        "end",
        "suspend",
    ];

    fn type_name(&self) -> &'static str {
        match self {
            Self::Init(_) => "init",
            Self::Context(_) => "context",
            Self::Theme(_) => "theme",
            Self::Changed(_) => "changed",
            Self::Visibility(_) => "visibility",
            Self::Response(_) => "response",
            Self::Heartbeat(_) => "heartbeat",
            Self::Chunk(_) => "chunk",
            Self::End(_) => "end",
            Self::Suspend(_) => "suspend",
        }
    }

    fn bytes_field(type_name: &str) -> Option<&'static str> {
        match type_name {
            "init" => Some("restored"),
            "response" | "chunk" => Some("body"),
            _ => None,
        }
    }

    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Init(init) => init.restored.as_deref(),
            Self::Response(response) => response.body.as_deref(),
            Self::Chunk(chunk) => chunk.body.as_deref(),
            _ => None,
        }
    }

    fn set_bytes(&mut self, bytes: Vec<u8>) {
        match self {
            Self::Init(init) => init.restored = Some(bytes),
            Self::Response(response) => response.body = Some(bytes),
            Self::Chunk(chunk) => chunk.body = Some(bytes),
            _ => {}
        }
    }

    fn take_bytes(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Init(init) => init.restored.take(),
            Self::Response(response) => response.body.take(),
            Self::Chunk(chunk) => chunk.body.take(),
            _ => None,
        }
    }
}

/// `init`: who the module is, and the world it is drawing in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Init {
    /// The platform whose module this is.
    pub platform: String,
    /// The panel key, or the page path for a page.
    pub panel: String,
    /// This mounted instance.
    pub instance: String,
    /// Whether the module is open as a full-width page rather than a panel.
    #[serde(default)]
    pub page: bool,
    /// The surface's time range and the panel's parameters.
    pub context: Context,
    /// The colour scheme and design tokens.
    pub theme: Theme,
    /// Who is looking, for display only.
    pub viewer: Viewer,
    /// Writes will be refused, so the module should stop offering them.
    #[serde(default)]
    pub read_only: bool,
    /// The limits in force for this platform.
    #[serde(default)]
    pub limits: Limits,
    /// What the module handed back at its last `suspend` on this page. An
    /// `ArrayBuffer` on the wire; `null` in the JSON when there is none.
    #[serde(skip_deserializing, serialize_with = "always_null")]
    pub restored: Option<Vec<u8>>,
}

fn always_null<S: Serializer, T>(_: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_none()
}

/// `context`, and `init.context`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Context {
    /// The surface's time range.
    #[serde(default)]
    pub time_range: Option<TimeRange>,
    /// Only the parameters this panel declares.
    #[serde(default)]
    pub params: Selections,
    /// Rises with every change, so a module can tell a stale answer from a
    /// fresh one.
    #[serde(default)]
    pub generation: u64,
}

/// A time range in milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Inclusive start.
    pub from_millis: i64,
    /// Exclusive end.
    pub to_millis: i64,
}

/// `theme`, and `init.theme`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Light or dark.
    pub scheme: Scheme,
    /// CSS custom properties (`--hlin-surface` and so on) to their values.
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

/// The colour scheme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// Light.
    #[default]
    Light,
    /// Dark.
    Dark,
}

/// Who is looking. A name for display and nothing a platform would authorize
/// on: the platform learns who is asking from the token on every request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    /// A display name.
    #[serde(default)]
    pub name: Option<String>,
}

/// `init.limits`: the values in force for this platform. A missing field is
/// the specification's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// The largest `fetch` body.
    #[serde(default = "default_request_bytes")]
    pub request_bytes: u64,
    /// The largest whole (not streamed) response body.
    #[serde(default = "default_response_bytes")]
    pub response_bytes: u64,
    /// Fetches in flight per frame, streams included.
    #[serde(default = "default_fetches_in_flight")]
    pub fetches_in_flight: u64,
    /// Open streamed responses per frame.
    #[serde(default = "default_streams")]
    pub streams: u64,
}

fn default_request_bytes() -> u64 {
    1024 * 1024
}
fn default_response_bytes() -> u64 {
    4 * 1024 * 1024
}
fn default_fetches_in_flight() -> u64 {
    8
}
fn default_streams() -> u64 {
    2
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: default_request_bytes(),
            response_bytes: default_response_bytes(),
            fetches_in_flight: default_fetches_in_flight(),
            streams: default_streams(),
        }
    }
}

/// `changed` from the shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellChanged {
    /// The panel the change concerns.
    pub panel: String,
    /// The selections it was made under.
    #[serde(default)]
    pub selections: Selections,
    /// Who said so.
    pub from: ChangeSource,
}

/// Where a `changed` came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeSource {
    /// The platform's own event stream.
    Platform,
    /// Another module of the same platform, after a write.
    Module,
}

/// `visibility`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visibility {
    /// Whether the panel is in view and the tab showing.
    pub visible: bool,
}

/// `response`, answering a `fetch` (whose id is the envelope's `re`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// The platform's status, or the shell's when it refused.
    pub status: u16,
    /// Only the headers the shell passes back.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// The whole body; `None` for a streamed response, whose body follows as
    /// `chunk`s. An `ArrayBuffer` on the wire.
    #[serde(skip)]
    pub body: Option<Vec<u8>>,
    /// `None` when the platform answered; why, when the shell refused itself.
    #[serde(default)]
    pub refusal: Option<Refusal>,
    /// The body follows as `chunk` messages.
    #[serde(default, skip_serializing_if = "is_false")]
    pub streaming: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Why the shell refused a request itself (specification HLIN-S-0007,
/// *Refusals*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The request did not come from the shell's page.
    NotFromShell,
    /// No session.
    NotSignedIn,
    /// The path is outside the platform's declared prefixes.
    OutsidePrefix,
    /// A method the bridge does not carry, or `stream` on a write.
    Method,
    /// The shell is read-only.
    ReadOnly,
    /// The platform cannot tell viewers apart, so it cannot take writes.
    NoIdentity,
    /// A write without an idempotency key.
    NoIdempotencyKey,
    /// A body over its limit.
    TooLarge,
    /// The platform could not be reached.
    Unreachable,
    /// The platform did not answer in time.
    Timeout,
    /// The frame's rate or count limit.
    TooMany,
    /// A code from a newer minor.
    #[serde(other)]
    Unrecognised,
}

/// `heartbeat`, both ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// The shell's count, echoed back.
    pub n: u64,
}

/// `chunk`: part of a streamed response body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// The id of the `fetch` this belongs to.
    pub re: String,
    /// From 0, in order.
    pub seq: u64,
    /// The bytes. An `ArrayBuffer` on the wire.
    #[serde(skip)]
    pub body: Option<Vec<u8>>,
}

/// `end`: a streamed response is over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct End {
    /// The id of the `fetch` this ends.
    pub re: String,
    /// Absent when the body ended normally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<EndError>,
}

/// Why a stream ended early.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndError {
    /// No bytes for `stream_idle_seconds`.
    Idle,
    /// Over `stream_bytes_per_second`.
    Rate,
    /// The platform went away.
    Unreachable,
    /// The module cancelled it.
    Cancelled,
    /// The frame was unmounted.
    Unmounted,
    /// A reason from a newer minor.
    #[serde(other)]
    Unrecognised,
}

/// `suspend`: the frame is about to be unmounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suspend {
    /// How long the page waits for `state`.
    pub deadline_ms: u64,
}

// ---------------------------------------------------------------------------
// Module to shell
// ---------------------------------------------------------------------------

/// A message from a module to the shell's page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "kebab-case")]
pub enum ModuleMessage {
    /// Once, when the module can draw.
    Ready(Ready),
    /// A request to the module's own platform.
    Fetch(Fetch),
    /// Select values for a parameter the panel declares.
    SetParam(SetParam),
    /// Set the surface's time range.
    SetRange(TimeRange),
    /// Open a page or panel in the shell.
    Navigate(Navigate),
    /// The module wrote something; tell the platform's other modules.
    Changed(ModuleChanged),
    /// A short message shown in the panel's frame, labelled as the platform's.
    Notice(Notice),
    /// The shell's heartbeat, echoed.
    Heartbeat(Heartbeat),
    /// Credit for more of a streamed body.
    Pull(Pull),
    /// End a streamed body early.
    Cancel(Cancel),
    /// What to keep across an unmount, answering `suspend`.
    State(State),
}

impl Message for ModuleMessage {
    const TYPES: &'static [&'static str] = &[
        "ready",
        "fetch",
        "set-param",
        "set-range",
        "navigate",
        "changed",
        "notice",
        "heartbeat",
        "pull",
        "cancel",
        "state",
    ];

    fn type_name(&self) -> &'static str {
        match self {
            Self::Ready(_) => "ready",
            Self::Fetch(_) => "fetch",
            Self::SetParam(_) => "set-param",
            Self::SetRange(_) => "set-range",
            Self::Navigate(_) => "navigate",
            Self::Changed(_) => "changed",
            Self::Notice(_) => "notice",
            Self::Heartbeat(_) => "heartbeat",
            Self::Pull(_) => "pull",
            Self::Cancel(_) => "cancel",
            Self::State(_) => "state",
        }
    }

    fn bytes_field(type_name: &str) -> Option<&'static str> {
        match type_name {
            "fetch" => Some("body"),
            "state" => Some("blob"),
            _ => None,
        }
    }

    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Fetch(fetch) => fetch.body.as_deref(),
            Self::State(state) => state.blob.as_deref(),
            _ => None,
        }
    }

    fn set_bytes(&mut self, bytes: Vec<u8>) {
        match self {
            Self::Fetch(fetch) => fetch.body = Some(bytes),
            Self::State(state) => state.blob = Some(bytes),
            _ => {}
        }
    }

    fn take_bytes(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Fetch(fetch) => fetch.body.take(),
            Self::State(state) => state.blob.take(),
            _ => None,
        }
    }
}

/// `ready`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    /// The shared kit the module was built against, as `name@version`. Logged
    /// by the shell, never judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<String>,
}

/// `fetch`: a request to the module's own platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fetch {
    /// The method.
    pub method: Method,
    /// Relative to the platform's base.
    pub path: String,
    /// The query string, without its `?`.
    #[serde(default)]
    pub query: String,
    /// Only [`crate::ALLOWED_REQUEST_HEADERS`]; anything else is dropped.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// The body. An `ArrayBuffer` on the wire.
    #[serde(skip)]
    pub body: Option<Vec<u8>>,
    /// Required on writes; the same across retries of one attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Send the response body as it arrives. Reads only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stream: bool,
}

/// An HTTP method the bridge carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    /// A read.
    Get,
    /// A read.
    Head,
    /// A write.
    Post,
    /// A write.
    Put,
    /// A write.
    Patch,
    /// A write.
    Delete,
    /// Anything else, which the page refuses with `method`.
    #[serde(other)]
    Other,
}

impl Method {
    /// `POST`, `PUT`, `PATCH` and `DELETE`: they need `Author`, an
    /// idempotency key, and are never retried by the shell.
    pub fn is_write(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch | Self::Delete)
    }

    /// `GET` and `HEAD`.
    pub fn is_read(self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }

    /// The method as HTTP writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Other => "OTHER",
        }
    }
}

/// `set-param`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetParam {
    /// A parameter the panel declares.
    pub id: String,
    /// The values to select.
    pub values: Vec<String>,
}

/// `navigate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Navigate {
    /// Where to go.
    pub to: Target,
}

/// A page or panel the shell can open. May name another platform:
/// navigation is the shell's, and opening a page grants nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// The platform.
    pub platform: String,
    /// A navigation entry's path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// A panel key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
}

/// `changed` from a module, after a write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleChanged {
    /// The panel the change concerns.
    pub panel: String,
    /// The selections it was made under.
    #[serde(default)]
    pub selections: Selections,
}

/// `notice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// How serious.
    pub level: NoticeLevel,
    /// Plain text, at most [`crate::NOTICE_MAX_CHARS`] characters.
    pub text: String,
}

/// How serious a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    /// For information.
    Info,
    /// Something is degraded.
    Warning,
    /// Something failed.
    Error,
}

/// `pull`: credit for more of a streamed body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pull {
    /// The id of the streamed `fetch`.
    pub re: String,
    /// How many more bytes the module will take.
    pub bytes: u64,
}

/// `cancel`: end a streamed body early.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancel {
    /// The id of the streamed `fetch`.
    pub re: String,
}

/// `state`, answering `suspend`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// What to hand back as `init.restored`. An `ArrayBuffer` on the wire.
    #[serde(skip)]
    pub blob: Option<Vec<u8>>,
}
