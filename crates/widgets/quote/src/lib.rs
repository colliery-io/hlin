//! Widget 7 of twenty ([[HLIN-I-0012]]): a quote, rotated.
//!
//! Not shared. Everybody sees the quote of the hour, which moves on by
//! itself on the hour; anyone may skip ahead, or pin one they like so it
//! stays, and that is theirs alone. Writes are announced, as every widget's
//! are, so the same person's other browser agrees.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/quote` | Your quote now, and whether it is pinned |
//! | `POST` | `/api/quote/next` | Skip to another |
//! | `PUT` | `/api/quote/pin` | Keep this one |
//! | `DELETE` | `/api/quote/pin` | Let it rotate again |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the quote as a one-row `table`.
//!
//! The rules:
//!
//! - The quote of the hour is the same for everybody who has not skipped.
//! - Skipping moves only your own quote on, and only while nothing is pinned.
//! - Pinning keeps the quote you are looking at, through every hour, until
//!   you unpin it; there is nothing to unpin if nothing is pinned.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::Serialize;
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "quote";

/// Every quote this widget has: what was said, and who said it.
pub const QUOTES: &[(&str, &str)] = &[
    (
        "Simplicity is prerequisite for reliability.",
        "Edsger W. Dijkstra",
    ),
    (
        "It is not that we have a short time to live, but that we waste a lot of it.",
        "Seneca",
    ),
    (
        "Premature optimization is the root of all evil.",
        "Donald Knuth",
    ),
    ("Well begun is half done.", "Aristotle"),
    (
        "If I had more time, I would have written a shorter letter.",
        "Blaise Pascal",
    ),
    (
        "Nothing is particularly hard if you divide it into small jobs.",
        "Henry Ford",
    ),
    ("Measure twice, cut once.", "Proverb"),
    (
        "The obstacle in the path becomes the path.",
        "After Marcus Aurelius",
    ),
    (
        "Plans are worthless, but planning is everything.",
        "Dwight D. Eisenhower",
    ),
    ("Make it work, make it right, make it fast.", "Kent Beck"),
    (
        "We are what we repeatedly do.",
        "Will Durant, after Aristotle",
    ),
    (
        "A ship in harbour is safe, but that is not what ships are built for.",
        "John A. Shedd",
    ),
];

/// Everybody's place in the rotation, by `sub`. Nobody is in here until they
/// skip or pin.
#[derive(Debug, Clone, Default)]
pub struct Quotes {
    readers: BTreeMap<String, Reader>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Reader {
    /// How far ahead of the hour they have skipped.
    skipped: usize,
    /// The quote they are keeping, if any.
    pinned: Option<usize>,
}

/// One person's quote, as they see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    /// Which quote, by its place in [`QUOTES`].
    pub id: usize,
    /// What was said.
    pub text: &'static str,
    /// Who said it.
    pub by: &'static str,
    /// Whether it is pinned.
    pub pinned: bool,
}

/// Hours since the epoch, which is what the rotation turns on.
pub fn hour_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 3600)
}

impl Quotes {
    fn reader(&self, who: &str) -> Reader {
        self.readers.get(who).copied().unwrap_or_default()
    }

    fn current(&self, who: &str, hour: u64) -> usize {
        let reader = self.reader(who);
        reader
            .pinned
            .unwrap_or_else(|| (hour as usize + reader.skipped) % QUOTES.len())
    }

    /// `who`'s quote in `hour`.
    pub fn shown(&self, who: &str, hour: u64) -> Shown {
        let id = self.current(who, hour);
        let (text, by) = QUOTES[id];
        Shown {
            id,
            text,
            by,
            pinned: self.reader(who).pinned.is_some(),
        }
    }

    /// Move `who` on to another quote.
    pub fn next(&mut self, who: &str) -> Result<(), Refusal> {
        let reader = self.readers.entry(who.to_string()).or_default();
        if reader.pinned.is_some() {
            return Err(Refusal::conflict("Unpin this quote to see another"));
        }
        reader.skipped = (reader.skipped + 1) % QUOTES.len();
        Ok(())
    }

    /// Keep the quote `who` is looking at in `hour`.
    pub fn pin(&mut self, who: &str, hour: u64) -> Result<(), Refusal> {
        let current = self.current(who, hour);
        let reader = self.readers.entry(who.to_string()).or_default();
        if reader.pinned.is_some() {
            return Err(Refusal::conflict("This quote is already pinned"));
        }
        reader.pinned = Some(current);
        Ok(())
    }

    /// Let `who`'s quote rotate again.
    pub fn unpin(&mut self, who: &str) -> Result<(), Refusal> {
        let reader = self.readers.entry(who.to_string()).or_default();
        reader
            .pinned
            .take()
            .map(|_| ())
            .ok_or_else(|| Refusal::conflict("No quote is pinned"))
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Quotes> {
    Widget {
        panel: PANEL,
        name: "Quote",
        icon: "quote",
        title: "Quote of the hour",
        description: "A quote, moving on every hour",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // It moves on, on the hour, without anybody writing.
            refresh_ms: Some(300_000),
            data: |quotes, claims| {
                Envelope::Records(as_table(&quotes.shown(&claims.sub, hour_now())))
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The quote as a one-row table, for the shell to draw where the module is
/// not.
fn as_table(shown: &Shown) -> Records {
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    let row = json!({ "quote": shown.text, "by": shown.by });
    Records {
        columns: vec![column("quote", "Quote"), column("by", "Who")],
        rows: vec![row.as_object().expect("an object").clone()],
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Quotes>> {
    Router::new()
        .route("/api/quote", get(read))
        .route("/api/quote/next", post(next))
        .route("/api/quote/pin", put(pin).delete(unpin))
}

async fn read(State(platform): State<Platform<Quotes>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|quotes| quotes.shown(&claims.sub, hour_now()))).into_response()
}

async fn next(State(platform): State<Platform<Quotes>>, write: Write) -> Response {
    platform.write(&write, |quotes, claims| {
        quotes.next(&claims.sub)?;
        Ok(Reply::ok(quotes.shown(&claims.sub, hour_now())))
    })
}

async fn pin(State(platform): State<Platform<Quotes>>, write: Write) -> Response {
    platform.write(&write, |quotes, claims| {
        quotes.pin(&claims.sub, hour_now())?;
        Ok(Reply::ok(quotes.shown(&claims.sub, hour_now())))
    })
}

async fn unpin(State(platform): State<Platform<Quotes>>, write: Write) -> Response {
    platform.write(&write, |quotes, claims| {
        quotes.unpin(&claims.sub)?;
        Ok(Reply::ok(quotes.shown(&claims.sub, hour_now())))
    })
}
