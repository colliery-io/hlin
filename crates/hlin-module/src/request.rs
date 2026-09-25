//! Requests to a module's own platform, and what comes back.

use std::collections::BTreeMap;

use hlin_bridge::{Method, Refusal, Response, is_allowed_request_header};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A request to the module's own platform.
///
/// The path is relative to the platform's base; the shell decides which
/// platform from the frame the request came from, never from anything here.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Option<Vec<u8>>,
}

impl Request {
    /// A request with any method the bridge carries.
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: String::new(),
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// A `GET`.
    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::Get, path)
    }

    /// A `HEAD`.
    pub fn head(path: impl Into<String>) -> Self {
        Self::new(Method::Head, path)
    }

    /// A `POST`.
    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::Post, path)
    }

    /// A `PUT`.
    pub fn put(path: impl Into<String>) -> Self {
        Self::new(Method::Put, path)
    }

    /// A `PATCH`.
    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::Patch, path)
    }

    /// A `DELETE`.
    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::Delete, path)
    }

    /// The query string, without its `?`.
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = query.into();
        self
    }

    /// A header. Only `content-type`, `accept`, `if-match` and
    /// `if-none-match` cross the bridge; any other is dropped here, where a
    /// developer can see it in a debugger, rather than silently by the page.
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        if is_allowed_request_header(name) {
            self.headers.insert(name.to_ascii_lowercase(), value.into());
        }
        self
    }

    /// A body and its content type.
    pub fn body(self, content_type: &str, bytes: impl Into<Vec<u8>>) -> Self {
        let mut this = self.header("content-type", content_type);
        this.body = Some(bytes.into());
        this
    }

    /// A JSON body.
    pub fn json<T: Serialize + ?Sized>(self, value: &T) -> Result<Self, serde_json::Error> {
        Ok(self.body("application/json", serde_json::to_vec(value)?))
    }

    /// The method.
    pub fn method(&self) -> Method {
        self.method
    }

    /// The path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// What came back from a `fetch`: the platform's answer, or the shell's
/// refusal. Kept apart because they mean different things to a person. The
/// platform's words are the module's to show as its own; a refusal is the
/// shell's, and usually means the module asked for something it should not
/// have (a path outside its prefixes, a write while read-only).
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// The platform answered, whatever its status.
    Answered(Answer),
    /// The shell refused the request itself.
    Refused(Refused),
}

impl Reply {
    /// The platform's answer, if it gave one.
    pub fn answered(&self) -> Option<&Answer> {
        match self {
            Self::Answered(answer) => Some(answer),
            Self::Refused(_) => None,
        }
    }

    /// The shell's refusal, if it made one.
    pub fn refused(&self) -> Option<&Refused> {
        match self {
            Self::Refused(refused) => Some(refused),
            Self::Answered(_) => None,
        }
    }

    pub(crate) fn from_response(response: Response) -> Self {
        match response.refusal {
            Some(refusal) => Self::Refused(Refused {
                refusal,
                status: response.status,
                reason: String::from_utf8_lossy(response.body.as_deref().unwrap_or_default())
                    .into_owned(),
            }),
            None => Self::Answered(Answer {
                status: response.status,
                headers: response.headers,
                body: response.body.unwrap_or_default(),
            }),
        }
    }
}

/// The platform's answer, passed back unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The platform's status.
    pub status: u16,
    /// `content-type`, `etag`, `last-modified` and `cache-control`, when the
    /// platform sent them.
    pub headers: BTreeMap<String, String>,
    /// The whole body.
    pub body: Vec<u8>,
}

impl Answer {
    /// A 2xx status.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text, replacing anything that is not UTF-8.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body as JSON.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// A request the shell refused before, or instead of, the platform answering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Why.
    pub refusal: Refusal,
    /// The shell's status for it.
    pub status: u16,
    /// A short reason the shell wrote. For a log or a developer, not for
    /// showing a person as the platform's words.
    pub reason: String,
}

/// Why a `fetch` produced no reply at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// The module went away before the answer came.
    Gone,
    /// `stream` was asked of a write, whose answer is a decision rather than a
    /// feed. The page would refuse it with `method`; the SDK does not send it.
    StreamOnWrite,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gone => f.write_str("the bridge went away before the answer came"),
            Self::StreamOnWrite => f.write_str("only a read can be streamed"),
        }
    }
}

impl std::error::Error for BridgeError {}
