//! The one thing a widget's components know about where their data comes
//! from: a [`Client`].
//!
//! Two implement it. `hlin_widget_ui::direct::Direct`, for the widget's own
//! UI at the root of its origin, calls `/api/` with `fetch` and hears of
//! changes on the widget's event stream. `hlin_widget_module::Hlin`, for the
//! module in a Hlin panel, goes through the bridge. The components never see
//! which: they use the [`Widget`](crate::Widget) handle, which is written once
//! over this trait.
//!
//! The types here are the requests and answers of the widget's own API, not of
//! either transport, so a components crate depends on neither.

use std::future::Future;
use std::pin::Pin;

use leptos::prelude::Signal;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A future a client hands back. Not `Send`: a page has one thread.
pub type Pending<T> = Pin<Box<dyn Future<Output = T>>>;

/// A future borrowing what it reads from.
pub type Borrowed<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Where a widget's components get their data, and what they are told.
///
/// Every path is the widget's own, starting `/api/`: the direct client sends
/// it to its own origin, and Hlin's sends it to the platform's base, which is
/// the `/hlin` subtree.
pub trait Client: 'static {
    /// Send a read now.
    fn fetch(&self, request: Request) -> Pending<Reply>;

    /// Prepare a write, holding one idempotency key for every time it is
    /// sent, so sending it again after a network failure is the same write.
    fn attempt(&self, request: Request) -> Box<dyn Attempt>;

    /// A read whose body arrives as it is written, for something that is
    /// followed rather than fetched (a log).
    fn stream(&self, request: Request) -> Pending<Streamed>;

    /// Tracked by everything the widget loads: it changes whenever what was
    /// loaded may be out of date. The widget's event stream said so, or (in
    /// Hlin) the surface's context changed or another of the platform's
    /// modules wrote something.
    fn changes(&self) -> Signal<u64>;

    /// A write happened: tell whoever else shows this widget's data. Hlin
    /// relays it to the platform's other modules on the page; the widget's
    /// own UI has nobody to tell, since the event stream reaches every other
    /// browser.
    fn announce(&self);

    /// Whether writes will be refused, so the widget can stop offering them.
    fn read_only(&self) -> Signal<bool>;

    /// Whether anybody can see the widget, so a ticking widget can stop.
    fn visible(&self) -> Signal<bool>;

    /// The time range the widget is asked to show, where there is one.
    fn time_range(&self) -> Signal<Option<TimeRange>>;

    /// What the widget kept when it was last suspended ([`Client::on_suspend`]),
    /// if it is being brought back.
    fn restored(&self) -> Option<Vec<u8>>;

    /// What to keep if the widget is suspended, asked for at the moment it is.
    /// Hlin suspends a module to save memory and brings it back later; a
    /// widget's own page is never suspended, so its client never asks.
    fn on_suspend(&self, keep: Box<dyn Fn() -> Option<Vec<u8>>>);
}

/// One write, which can be sent again with the same idempotency key.
pub trait Attempt {
    /// Send it, the first time or again.
    fn send(&self) -> Pending<Reply>;
}

/// What a request came back with: the platform's answer, whatever its status,
/// or why there was none.
pub type Reply = Result<Answer, Trouble>;

/// Why a request got no answer from the platform, in words for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trouble {
    /// What to tell the person.
    pub words: String,
    /// Whether the same request might work if sent again: the network, not a
    /// rule.
    pub retry: bool,
}

/// A request's method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `PATCH`.
    Patch,
    /// `DELETE`.
    Delete,
}

impl Method {
    /// As HTTP writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// A request to the widget's own API.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The method.
    pub method: Method,
    /// The path, from `/api/`.
    pub path: String,
    /// The query, without its `?`; empty for none.
    pub query: String,
    /// The body's type, where there is a body.
    pub content_type: Option<String>,
    /// The body.
    pub body: Option<Vec<u8>>,
}

impl Request {
    /// A request with this method.
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: String::new(),
            content_type: None,
            body: None,
        }
    }

    /// A `GET`.
    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::Get, path)
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

    /// A JSON body.
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Result<Self, serde_json::Error> {
        self.body = Some(serde_json::to_vec(value)?);
        self.content_type = Some("application/json".to_string());
        Ok(self)
    }

    /// The path and the query, as a URL relative to wherever the API is.
    pub fn target(&self) -> String {
        if self.query.is_empty() {
            self.path.clone()
        } else {
            format!("{}?{}", self.path, self.query)
        }
    }
}

/// The platform's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The status.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
}

impl Answer {
    /// Whether it is a 2xx.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as JSON of this shape.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// What a [`Client::stream`] came back with.
pub enum Streamed {
    /// A 2xx, whose body arrives in chunks as it is written.
    Streaming(Box<dyn Chunks>),
    /// Any other answer, gathered whole, so it can be said in the platform's
    /// words.
    Answered(Answer),
    /// No answer.
    Trouble(Trouble),
}

/// A body arriving in chunks. Chunks are not lines or records: they fall
/// however the network delivered them.
pub trait Chunks {
    /// The next chunk; `None` at the end of the body; why it stopped short
    /// otherwise.
    fn next(&mut self) -> Borrowed<'_, Result<Option<Vec<u8>>, Ended>>;
}

/// Why a stream stopped before its body ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Nothing arrived for too long.
    Idle,
    /// It sent more than it was allowed.
    Rate,
    /// The platform stopped answering.
    Unreachable,
    /// The widget itself is going (its panel or page closed).
    Gone,
}

/// A time range: from inclusive, to exclusive, in epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    /// Inclusive start.
    pub from_millis: i64,
    /// Exclusive end.
    pub to_millis: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_with_a_query_targets_its_path_and_query() {
        let request = Request::get("/api/deploys/log").query("after=4");
        assert_eq!(request.target(), "/api/deploys/log?after=4");
        assert_eq!(Request::get("/api/poll").target(), "/api/poll");
    }

    #[test]
    fn a_json_body_says_it_is_json() {
        let request = Request::post("/api/counter/bump")
            .json(&serde_json::json!({ "by": 1 }))
            .unwrap();
        assert_eq!(request.method.as_str(), "POST");
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        assert_eq!(request.body.as_deref(), Some(&br#"{"by":1}"#[..]));
    }
}
