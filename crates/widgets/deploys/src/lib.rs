//! Widget 18 of twenty ([[HLIN-I-0012]]): a deploy log, streamed.
//!
//! The first widget whose module reads a body as it arrives (specification
//! HLIN-S-0007, *Streaming*; [[HLIN-T-0068]]). The log is one long read,
//! `GET /api/deploys/log`, which answers the last few lines at once and then a
//! line whenever the next one is due, and never finishes by itself. The module
//! asks for it with `fetch_stream` and reads it with the SDK's `BodyReader`,
//! which asks for more only as it hands lines over; the shell passes the body
//! through as it comes, so a module that stops reading stops this platform
//! writing. When the person scrolls it away or closes the page, the shell lets
//! go of the connection, and the log's body is dropped here: nothing is left
//! running for a reader who has gone.
//!
//! The deploys are synthetic, and the same for everybody: line `n` is a pure
//! function of `n` ([`line`]), due at `n` ticks after the Unix epoch. So two
//! browsers show the same log, a restart does not change history, and a
//! module that reconnects can say where it got to (`?after=`) and carry on
//! without a gap or a repeat.
//!
//! Not shared, in the table's sense: nobody changes anything here. There are
//! no writes, so there is nothing to announce, and the event stream (which
//! every widget has) stays quiet. The log is the module's own data, not a
//! second channel for `changed` (HLIN-S-0007: liveness is unchanged).
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/deploys/log` | The log, as newline-delimited JSON, as it happens. `?after=n` starts after line `n` |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the recent lines as a `table`, which the shell refetches
//! every few seconds where the module cannot stream.
//!
//! The rules:
//!
//! - Anyone signed in may read the log; nobody may change it.
//! - A reader is first sent the last [`RECENT`] lines, or fewer if it says
//!   where it got to, then each line as it is due, in order, with no gaps.
//! - The log is written only as the reader takes it, and stops when the
//!   reader goes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{DateTime, Utc};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Fallback, Platform, Viewer, Widget};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "deploys";

/// How many lines a new reader is sent at once, and the fallback shows.
pub const RECENT: u64 = 12;

/// How often a line is due, unless the widget is told otherwise.
pub const EVERY: Duration = Duration::from_secs(1);

/// The services that deploy.
pub const SERVICES: &[&str] = &["api", "web", "billing", "search", "auth", "worker"];

/// The people who deploy them.
pub const PEOPLE: &[&str] = &["Ada", "Grace", "Linus", "Margaret", "Ken", "Barbara"];

/// Lines per deploy: started, built, rolling out, and how it ended.
const STEPS: u64 = 4;

/// The log: how often a line is due, and how many readers are holding it
/// open, which is how the tests see a reader's going reach the platform.
#[derive(Debug, Clone)]
pub struct Deploys {
    every: Duration,
    open: Arc<AtomicUsize>,
}

impl Default for Deploys {
    fn default() -> Self {
        Self::every(EVERY)
    }
}

impl Deploys {
    /// A log with a line due every `every`.
    pub fn every(every: Duration) -> Self {
        Self {
            every: every.max(Duration::from_millis(1)),
            open: Arc::default(),
        }
    }

    /// How many readers are holding the log open now.
    pub fn open_readers(&self) -> usize {
        self.open.load(Ordering::SeqCst)
    }

    fn every_ms(&self) -> u64 {
        u64::try_from(self.every.as_millis()).unwrap_or(u64::MAX)
    }

    /// The number of the latest line due at `now`.
    pub fn latest(&self, now: DateTime<Utc>) -> u64 {
        u64::try_from(now.timestamp_millis()).unwrap_or(0) / self.every_ms()
    }

    /// When line `seq` is due.
    pub fn due(&self, seq: u64) -> DateTime<Utc> {
        let millis = seq.saturating_mul(self.every_ms());
        DateTime::from_timestamp_millis(i64::try_from(millis).unwrap_or(i64::MAX))
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    }

    /// Line `seq`, stamped with when it was due.
    pub fn line(&self, seq: u64) -> Line {
        line(seq, self.due(seq))
    }

    /// The first line a reader is sent at `now`: the last [`RECENT`], or the
    /// one after `after` if that is later.
    pub fn first_for(&self, after: Option<u64>, now: DateTime<Utc>) -> u64 {
        let latest = self.latest(now);
        let recent = (latest + 1).saturating_sub(RECENT);
        match after {
            Some(after) => (after + 1).clamp(recent, latest + 1),
            None => recent,
        }
    }
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// Its number. Each is one more than the last.
    pub seq: u64,
    /// When it happened.
    pub at: DateTime<Utc>,
    /// Which service.
    pub service: String,
    /// Which version of it.
    pub version: String,
    /// `info`, `good` or `bad`, for colouring.
    pub level: String,
    /// What happened, in words.
    pub text: String,
}

/// A scrambling of `n`, so neighbouring deploys differ (SplitMix64's finish).
fn mix(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Line `seq` of the log, at `at`: the same words for everybody, always.
pub fn line(seq: u64, at: DateTime<Utc>) -> Line {
    let deploy = seq / STEPS;
    let bits = mix(deploy);
    let pick = |list: &'static [&'static str], shift: u32| {
        list[((bits >> shift) % list.len() as u64) as usize]
    };
    let service = pick(SERVICES, 0);
    let who = pick(PEOPLE, 8);
    let place = if (bits >> 16).is_multiple_of(3) {
        "staging"
    } else {
        "production"
    };
    let failed = (bits >> 24).is_multiple_of(6);
    let version = format!("1.{}.{}", (deploy / 37) % 20, deploy % 37);

    let (level, text) = match seq % STEPS {
        0 => ("info", format!("deploy to {place} started by {who}")),
        1 => (
            "info",
            format!("built and tested in {}s", 20 + (bits >> 32) % 70),
        ),
        2 => ("info", format!("rolling out to {place}")),
        _ if failed => ("bad", "health checks failed, rolled back".to_string()),
        _ => ("good", format!("live on {place}")),
    };
    Line {
        seq,
        at,
        service: service.to_string(),
        version,
        level: level.to_string(),
        text,
    }
}

/// What a reader may ask of the log.
#[derive(Debug, Default, Deserialize)]
pub struct Asked {
    /// The last line the reader already has, to carry on after it.
    pub after: Option<u64>,
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Deploys> {
    Widget {
        panel: PANEL,
        name: "Deploys",
        icon: "rocket",
        title: "Deploys",
        description: "The deploy log, as it happens",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // New lines arrive without anybody writing, so the shell has to
            // ask again. Not every second: the stream is for that.
            refresh_ms: Some(5_000),
            data: |deploys, _| Envelope::Records(as_table(deploys, Utc::now())),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The recent lines as a table, for the shell to draw where the module is not.
fn as_table(deploys: &Deploys, now: DateTime<Utc>) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    let first = deploys.first_for(None, now);
    Records {
        columns: vec![
            column("at", "When", ColumnType::Timestamp),
            column("service", "Service", ColumnType::String),
            column("version", "Version", ColumnType::String),
            column("text", "What", ColumnType::String),
        ],
        // Newest first, as a person reads a log that is not moving.
        rows: (first..=deploys.latest(now))
            .rev()
            .map(|seq| {
                let line = deploys.line(seq);
                let row = json!({
                    "at": line.at.to_rfc3339(),
                    "service": line.service,
                    "version": line.version,
                    "text": line.text,
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: Some(now),
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Deploys>> {
    Router::new().route("/api/deploys/log", get(log))
}

/// Counts a reader for as long as its body lives, which is as long as its
/// connection does, however it ends.
struct Reading(Arc<AtomicUsize>);

impl Reading {
    fn new(open: Arc<AtomicUsize>) -> Self {
        open.fetch_add(1, Ordering::SeqCst);
        Self(open)
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The log, as newline-delimited JSON, one line per chunk, for as long as the
/// reader stays.
///
/// Each line is written only when the connection takes the one before it, so
/// a reader that stops reading stops the log; and when the connection goes,
/// the body is dropped at its next line and nothing more is written.
async fn log(
    State(platform): State<Platform<Deploys>>,
    Viewer(_): Viewer,
    Query(asked): Query<Asked>,
) -> Response {
    let deploys = platform.read(Deploys::clone);
    let reading = Reading::new(deploys.open.clone());
    let mut next = deploys.first_for(asked.after, Utc::now());

    let body = async_stream::stream! {
        let _reading = reading;
        loop {
            let wait = (deploys.due(next) - Utc::now()).to_std().unwrap_or_default();
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let mut text = serde_json::to_string(&deploys.line(next)).expect("a line serialises");
            text.push('\n');
            yield Ok::<_, std::convert::Infallible>(text);
            next += 1;
        }
    };

    (
        [
            ("content-type", "application/x-ndjson"),
            ("cache-control", "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}
