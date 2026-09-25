//! A feed of this platform's own, for its module to stream (specification
//! HLIN-S-0007, *Streaming*).
//!
//! What a module streams is the platform's business — a log tail, an export,
//! a server-sent event source of its own — so this is the plainest of those:
//! numbered `tick` events at a pace the caller names. It is not the
//! platform's event stream, which the shell alone subscribes to; it is data,
//! read through the shell like any other read under [`crate::modules::READS`].
//!
//! Each feed may be given an id, and then keeps count of what it has written
//! and whether its connection is still open, for anyone to ask. That is what
//! a browser test needs to see from the far end: that a module which stopped
//! pulling stopped the platform too, and that a cancelled or unmounted stream
//! reached the platform as a dropped connection.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// How many feeds' counts are remembered. A demo runs for days, and nothing
/// needs the count of a feed from long ago.
const REMEMBERED: usize = 256;

/// The largest padding a tick may carry, so a caller cannot ask this platform
/// to build one enormous event.
const MAX_PADDING: usize = 64 * 1024;

/// What a caller may ask of a feed.
#[derive(Debug, Deserialize)]
pub struct Asked {
    /// A name to count this feed under, for [`counts`].
    pub id: Option<String>,
    /// Milliseconds between ticks. The first comes at once.
    #[serde(default = "default_every_ms")]
    pub every_ms: u64,
    /// Bytes of padding in each tick, to make a feed heavy.
    #[serde(default)]
    pub bytes: usize,
    /// How many ticks before the feed ends by itself.
    pub count: Option<u64>,
}

fn default_every_ms() -> u64 {
    250
}

/// What a feed has done, as [`counts`] reports it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    /// Ticks written to the connection.
    pub ticks: u64,
    /// Bytes written to the connection.
    pub bytes: u64,
    /// Whether the connection is still open.
    pub open: bool,
}

#[derive(Default)]
struct Ledger {
    counts: BTreeMap<String, Counts>,
    order: VecDeque<String>,
}

static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(Mutex::default);

fn record(id: &str, change: impl FnOnce(&mut Counts)) {
    let mut ledger = LEDGER.lock().expect("the ledger is never poisoned");
    if !ledger.counts.contains_key(id) {
        ledger.order.push_back(id.to_string());
        if ledger.order.len() > REMEMBERED
            && let Some(oldest) = ledger.order.pop_front()
        {
            ledger.counts.remove(&oldest);
        }
    }
    change(ledger.counts.entry(id.to_string()).or_default());
}

/// Marks a feed closed when its body is dropped, which is when the
/// connection goes, however it goes.
struct Open(Option<String>);

impl Drop for Open {
    fn drop(&mut self) {
        if let Some(id) = &self.0 {
            record(id, |counts| counts.open = false);
        }
    }
}

/// The feed itself, as `text/event-stream`.
///
/// Each tick is written only when the connection takes it, so a reader that
/// stops reading stops the count: the ledger shows backpressure arriving.
pub fn stream(asked: Asked) -> Response {
    let every = Duration::from_millis(asked.every_ms.clamp(1, 60_000));
    let padding = "x".repeat(asked.bytes.min(MAX_PADDING));
    let count = asked.count.unwrap_or(u64::MAX);
    let open = Open(asked.id);
    if let Some(id) = &open.0 {
        record(id, |counts| {
            *counts = Counts {
                open: true,
                ..Counts::default()
            }
        });
    }

    let body = async_stream::stream! {
        let open = open;
        for n in 0..count {
            if n > 0 {
                tokio::time::sleep(every).await;
            }
            let event = if padding.is_empty() {
                format!("id: {n}\ndata: tick {n}\n\n")
            } else {
                format!("id: {n}\ndata: tick {n} {padding}\n\n")
            };
            if let Some(id) = &open.0 {
                record(id, |counts| {
                    counts.ticks += 1;
                    counts.bytes += event.len() as u64;
                });
            }
            yield Ok::<_, std::convert::Infallible>(event);
        }
    };

    (
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

/// What the feed named `id` has done, or 404 if no feed has had that name
/// lately.
pub fn counts(id: &str) -> Response {
    let ledger = LEDGER.lock().expect("the ledger is never poisoned");
    match ledger.counts.get(id) {
        Some(counts) => Json(counts.clone()).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_latest_feeds_are_remembered() {
        for n in 0..REMEMBERED + 10 {
            record(&format!("remembered-{n}"), |counts| counts.ticks = n as u64);
        }
        let ledger = LEDGER.lock().unwrap();
        assert!(ledger.counts.len() <= REMEMBERED);
        assert!(!ledger.counts.contains_key("remembered-0"));
        assert!(
            ledger
                .counts
                .contains_key(&format!("remembered-{}", REMEMBERED + 9))
        );
    }
}
