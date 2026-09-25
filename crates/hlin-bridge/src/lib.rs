#![warn(missing_docs)]

//! The Hlin module bridge, on the wire.
//!
//! A platform's module runs in a sandboxed frame and talks to the shell's page
//! over `postMessage` (specification HLIN-S-0007, *Messages*). This crate is
//! every message that crosses, in both directions, as Rust types that
//! serialise to exactly the JSON the specification shows. The shell's page
//! reads it to host frames; `hlin-module` reads it to be one. One definition
//! for both ends is the point: the protocol cannot drift between them if there
//! is only one of it.
//!
//! Its own crate rather than a module of `hlin-stream`, because the two have
//! different readers. `hlin-stream` is what the shell tells its own browser,
//! and it pulls in the view registry and the whole manifest contract to say
//! it. This is what a *platform team's* module compiles, and it should cost
//! them `serde` and nothing else of Hlin's. It is also a public contract with
//! its own version, `[major, minor]`, published on its own schedule.
//!
//! # Bytes
//!
//! Request and response bodies, `state` blobs and `init.restored` travel as
//! `ArrayBuffer`s, transferred rather than copied. JSON has no bytes, so every
//! such field is `Option<Vec<u8>>` here and left out of the serde form: the
//! JSON is the rest of the message, and the bytes ride beside it. The `js`
//! feature ([`js::to_js`], [`js::from_js`]) puts them back in as a transferable
//! `ArrayBuffer`, and takes them out of one, which is what both the page and
//! the SDK call.
//!
//! # Evolution
//!
//! Unknown fields are ignored and an unknown message type decodes to
//! [`Decoded::Unknown`] rather than an error (REQ-2.3), so a minor version can
//! add either without a coordinated release.
//!
//! ```
//! use hlin_bridge::{Decoded, Envelope, ModuleMessage, Ready, ShellMessage};
//!
//! // What a module says once it can draw.
//! let ready = Envelope::new("m-1", ModuleMessage::Ready(Ready { kit: Some("aurora@0.2.1".into()) }));
//! assert_eq!(
//!     ready.to_json(),
//!     serde_json::json!({ "bridge": [1, 0], "id": "m-1", "type": "ready",
//!                         "data": { "kit": "aurora@0.2.1" } }),
//! );
//!
//! // What a module hears, and what it does with a type from a newer minor.
//! let heard = serde_json::json!({ "bridge": [1, 3], "id": "s-9", "type": "sparkle", "data": {} });
//! assert!(matches!(Envelope::<ShellMessage>::from_json(&heard), Decoded::Unknown { .. }));
//! ```

#[cfg(feature = "js")]
pub mod js;
mod messages;

pub use messages::*;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// The bridge this crate speaks, `[major, minor]`, sent in every message.
pub const VERSION: [u32; 2] = [MAJOR, MINOR];

/// The bridge major. A module declares it in its manifest (`ui.bridge`) and
/// repeats it in every message; a mismatch makes its panel
/// `unavailable (malformed)`.
pub const MAJOR: u32 = 1;

/// The bridge minor. Additive only: any minor of a major works with any other.
pub const MINOR: u32 = 0;

/// The majors the shell can host, from the manifest crate so that the shell's
/// check and this crate cannot disagree about which majors exist.
pub use hlin_manifest::SUPPORTED_BRIDGE_MAJORS;

/// The request headers a module's `fetch` may carry. Anything else is dropped,
/// by the SDK before sending and by the page on receipt.
pub const ALLOWED_REQUEST_HEADERS: &[&str] =
    &["content-type", "accept", "if-match", "if-none-match"];

/// The platform response headers the shell passes back to a module.
pub const ALLOWED_RESPONSE_HEADERS: &[&str] =
    &["content-type", "etag", "last-modified", "cache-control"];

/// Whether a module may send this request header. Case-insensitive, because
/// HTTP header names are.
pub fn is_allowed_request_header(name: &str) -> bool {
    ALLOWED_REQUEST_HEADERS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(name))
}

/// The longest `notice` text the shell shows, in characters.
pub const NOTICE_MAX_CHARS: usize = 140;

/// How often the page sends `heartbeat` while a frame is visible.
pub const HEARTBEAT_INTERVAL_MS: u64 = 2_000;

/// How long the page waits for `ready` after a frame loads.
pub const READY_TIMEOUT_MS: u64 = 10_000;

/// The deadline the page offers in `suspend`.
pub const SUSPEND_DEADLINE_MS: u64 = 500;

/// The largest `state` blob the page keeps, unless an operator says otherwise.
pub const DEFAULT_STATE_BYTES: u64 = 64 * 1024;

/// One message with its envelope: who sent it, which message it answers, and
/// the bridge version the sender speaks.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<M> {
    /// `[major, minor]` the sender speaks.
    pub bridge: [u32; 2],
    /// Unique per sender for the life of the frame.
    pub id: String,
    /// On a reply, the `id` it answers.
    pub re: Option<String>,
    /// The message itself: its `type` and `data`.
    pub message: M,
}

impl<M: Message> Envelope<M> {
    /// A message in this crate's version.
    pub fn new(id: impl Into<String>, message: M) -> Self {
        Self {
            bridge: VERSION,
            id: id.into(),
            re: None,
            message,
        }
    }

    /// A reply to the message whose id is `re`.
    pub fn reply(id: impl Into<String>, re: impl Into<String>, message: M) -> Self {
        Self {
            re: Some(re.into()),
            ..Self::new(id, message)
        }
    }

    /// The message as JSON, without its bytes (see *Bytes* above).
    pub fn to_json(&self) -> Value {
        let (type_name, data) = self.message.to_parts();
        let mut object = Map::new();
        object.insert("bridge".into(), Value::from(self.bridge.to_vec()));
        object.insert("id".into(), Value::from(self.id.clone()));
        if let Some(re) = &self.re {
            object.insert("re".into(), Value::from(re.clone()));
        }
        object.insert("type".into(), Value::from(type_name));
        object.insert("data".into(), data);
        Value::Object(object)
    }

    /// Reads a message from JSON. Never fails: something that is not an
    /// envelope is [`Decoded::Malformed`], and a type this crate does not know
    /// is [`Decoded::Unknown`], both of which the reader drops without reply.
    pub fn from_json(value: &Value) -> Decoded<M> {
        let Some(object) = value.as_object() else {
            return Decoded::Malformed("not an object".into());
        };
        let bridge = match object.get("bridge").and_then(Value::as_array) {
            Some(parts) if parts.len() == 2 => match (parts[0].as_u64(), parts[1].as_u64()) {
                (Some(major), Some(minor))
                    if major <= u32::MAX as u64 && minor <= u32::MAX as u64 =>
                {
                    [major as u32, minor as u32]
                }
                _ => return Decoded::Malformed("bridge is not [major, minor]".into()),
            },
            _ => return Decoded::Malformed("bridge is not [major, minor]".into()),
        };
        let Some(id) = object.get("id").and_then(Value::as_str) else {
            return Decoded::Malformed("no id".into());
        };
        let re = match object.get("re") {
            None | Some(Value::Null) => None,
            Some(Value::String(re)) => Some(re.clone()),
            Some(_) => return Decoded::Malformed("re is not a string".into()),
        };
        let Some(type_name) = object.get("type").and_then(Value::as_str) else {
            return Decoded::Malformed("no type".into());
        };
        let data = match object.get("data") {
            Some(data @ Value::Object(_)) => data.clone(),
            _ => return Decoded::Malformed("data is not an object".into()),
        };
        if !M::TYPES.contains(&type_name) {
            return Decoded::Unknown {
                bridge,
                id: id.to_string(),
                type_name: type_name.to_string(),
            };
        }
        match M::from_parts(type_name, data) {
            Ok(message) => Decoded::Message(Envelope {
                bridge,
                id: id.to_string(),
                re,
                message,
            }),
            Err(error) => Decoded::Malformed(format!("{type_name}: {error}")),
        }
    }
}

/// What reading one message produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded<M> {
    /// A message this crate knows.
    Message(Envelope<M>),
    /// A well-formed envelope of a type this crate does not know, most likely
    /// from a newer minor. Ignored (REQ-2.3).
    Unknown {
        /// The sender's version.
        bridge: [u32; 2],
        /// The sender's id for it.
        id: String,
        /// The type it named.
        type_name: String,
    },
    /// Not an envelope, or a known type whose fields do not fit. Dropped
    /// without reply. The string is for a log, not for a person.
    Malformed(String),
}

impl<M> Decoded<M> {
    /// The message, if it was one this crate knows.
    pub fn message(self) -> Option<Envelope<M>> {
        match self {
            Self::Message(envelope) => Some(envelope),
            _ => None,
        }
    }
}

/// One direction's set of messages. Implemented by [`ShellMessage`] and
/// [`ModuleMessage`]; the generic envelope code and the `js` conversion are
/// written once against it.
pub trait Message: Serialize + DeserializeOwned {
    /// Every `type` this direction knows.
    const TYPES: &'static [&'static str];

    /// This message's `type`.
    fn type_name(&self) -> &'static str;

    /// The field of `data` that carries bytes for this `type`, if any.
    fn bytes_field(type_name: &str) -> Option<&'static str>;

    /// This message's bytes, if it carries any.
    fn bytes(&self) -> Option<&[u8]>;

    /// Puts bytes read off the wire back into the message. Ignored by a type
    /// that carries none.
    fn set_bytes(&mut self, bytes: Vec<u8>);

    /// Takes the bytes out, leaving `None`, so a sender can transfer them
    /// without a copy.
    fn take_bytes(&mut self) -> Option<Vec<u8>>;

    /// `type` and `data`, without the bytes.
    fn to_parts(&self) -> (&'static str, Value) {
        let value = serde_json::to_value(self).expect("a message always serialises");
        let data = match value {
            Value::Object(mut object) => object.remove("data").unwrap_or(Value::Null),
            _ => Value::Null,
        };
        (self.type_name(), data)
    }

    /// The message named by `type_name` from its `data`.
    fn from_parts(type_name: &str, data: Value) -> Result<Self, serde_json::Error> {
        let mut object = Map::new();
        object.insert("type".into(), Value::from(type_name));
        object.insert("data".into(), data);
        serde_json::from_value(Value::Object(object))
    }
}

/// Mints envelope ids: a prefix and a counter, unique for the life of a frame.
///
/// The specification's examples use `m-` for a module and `s-` for the shell,
/// which makes a log of both directions readable at a glance.
#[derive(Debug, Clone)]
pub struct IdMint {
    prefix: &'static str,
    next: u64,
}

impl IdMint {
    /// Ids for a module: `m-1`, `m-2`, …
    pub fn module() -> Self {
        Self {
            prefix: "m-",
            next: 1,
        }
    }

    /// Ids for the shell's page: `s-1`, `s-2`, …
    pub fn shell() -> Self {
        Self {
            prefix: "s-",
            next: 1,
        }
    }

    /// The next id.
    pub fn mint(&mut self) -> String {
        let id = format!("{}{}", self.prefix, self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests;
