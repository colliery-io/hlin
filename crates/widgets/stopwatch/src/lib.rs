//! Widget 6 of twenty ([[HLIN-I-0012]]): a per-person stopwatch.
//!
//! Not shared: everybody has their own, and starting yours starts nobody
//! else's. It lives on the platform rather than in the browser so it keeps
//! running while its panel is scrolled out of view and unmounted, and reads
//! the same in your other browser. Writes are announced, as every widget's
//! are, so that other browser follows; the panel does not declare `pushed`.
//!
//! The platform keeps when it was started and how much it had banked before;
//! the module adds the browser's own clock to that between fetches, so it
//! ticks without asking.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/stopwatch` | Yours: elapsed, running, laps |
//! | `POST` | `/api/stopwatch/start` | Start, or carry on |
//! | `POST` | `/api/stopwatch/stop` | Stop |
//! | `POST` | `/api/stopwatch/lap` | Mark a lap |
//! | `POST` | `/api/stopwatch/reset` | Back to zero |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the elapsed time as a `stat`.
//!
//! The rules:
//!
//! - Start only when stopped; stop and lap only when running.
//! - Reset only when stopped, so a running time is not thrown away by a
//!   stray click.
//! - At most [`MOST_LAPS`] laps.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hlin_widget_support::envelope::{Envelope, Scalar};
use hlin_widget_support::{Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::Serialize;
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "stopwatch";

/// The most laps one run may have.
pub const MOST_LAPS: usize = 20;

/// Everybody's stopwatches, by `sub`. Nobody is in here until they start one.
#[derive(Debug, Clone, Default)]
pub struct Stopwatches {
    watches: BTreeMap<String, Watch>,
}

/// One person's stopwatch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Watch {
    /// When it was last started, in epoch milliseconds, while it runs.
    since: Option<i64>,
    /// What it had counted before that, in milliseconds.
    banked: u64,
    /// The elapsed time at each lap, in milliseconds.
    laps: Vec<u64>,
}

/// A stopwatch as its owner sees it at one instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Face {
    /// Milliseconds counted, at `as_of`.
    pub elapsed_ms: u64,
    /// Whether it is counting.
    pub running: bool,
    /// The laps, first first.
    pub laps: Vec<Lap>,
    /// When this was true, in epoch milliseconds: the module counts on from
    /// here with its own clock.
    pub as_of: i64,
}

/// One lap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lap {
    /// Its number, from one.
    pub number: usize,
    /// The elapsed time when it was marked.
    pub at_ms: u64,
    /// How long the lap itself took.
    pub split_ms: u64,
}

impl Watch {
    fn elapsed(&self, now: i64) -> u64 {
        let running = self
            .since
            .map_or(0, |since| u64::try_from(now - since).unwrap_or(0));
        self.banked + running
    }
}

/// The time now, in epoch milliseconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

impl Stopwatches {
    fn watch(&mut self, who: &str) -> &mut Watch {
        self.watches.entry(who.to_string()).or_default()
    }

    /// `who`'s stopwatch as it reads at `now`.
    pub fn face(&self, who: &str, now: i64) -> Face {
        let watch = self.watches.get(who).cloned().unwrap_or_default();
        let mut before = 0;
        Face {
            elapsed_ms: watch.elapsed(now),
            running: watch.since.is_some(),
            laps: watch
                .laps
                .iter()
                .enumerate()
                .map(|(index, &at_ms)| {
                    let lap = Lap {
                        number: index + 1,
                        at_ms,
                        split_ms: at_ms - before,
                    };
                    before = at_ms;
                    lap
                })
                .collect(),
            as_of: now,
        }
    }

    /// Start `who`'s stopwatch at `now`, carrying on from what it had.
    pub fn start(&mut self, who: &str, now: i64) -> Result<(), Refusal> {
        let watch = self.watch(who);
        if watch.since.is_some() {
            return Err(Refusal::conflict("Your stopwatch is already running"));
        }
        watch.since = Some(now);
        Ok(())
    }

    /// Stop it at `now`.
    pub fn stop(&mut self, who: &str, now: i64) -> Result<(), Refusal> {
        let watch = self.watch(who);
        if watch.since.is_none() {
            return Err(Refusal::conflict("Your stopwatch is not running"));
        }
        watch.banked = watch.elapsed(now);
        watch.since = None;
        Ok(())
    }

    /// Mark a lap at `now`.
    pub fn lap(&mut self, who: &str, now: i64) -> Result<(), Refusal> {
        let watch = self.watch(who);
        if watch.since.is_none() {
            return Err(Refusal::conflict("Start the stopwatch to mark a lap"));
        }
        if watch.laps.len() >= MOST_LAPS {
            return Err(Refusal::conflict(format!(
                "{MOST_LAPS} laps is the most; reset to start again"
            )));
        }
        let at = watch.elapsed(now);
        watch.laps.push(at);
        Ok(())
    }

    /// Back to zero, laps and all.
    pub fn reset(&mut self, who: &str) -> Result<(), Refusal> {
        let watch = self.watch(who);
        if watch.since.is_some() {
            return Err(Refusal::conflict("Stop the stopwatch before resetting it"));
        }
        *watch = Watch::default();
        Ok(())
    }
}

/// `1:02:03.4`, `2:03.4`: how a stopwatch reads.
pub fn reading(ms: u64) -> String {
    let tenths = ms / 100 % 10;
    let seconds = ms / 1000 % 60;
    let minutes = ms / 60_000 % 60;
    let hours = ms / 3_600_000;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{tenths}")
    } else {
        format!("{minutes}:{seconds:02}.{tenths}")
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Stopwatches> {
    Widget {
        panel: PANEL,
        name: "Stopwatch",
        icon: "timer",
        title: "Stopwatch",
        description: "Your own stopwatch, which keeps running while you look elsewhere",
        shared: false,
        fallback: Some(Fallback {
            kind: "stat",
            envelope: "scalar.v1",
            // A running stopwatch moves on without anybody writing; every five
            // seconds is enough for a view that cannot be clicked anyway.
            refresh_ms: Some(5_000),
            data: |watches, claims| {
                let face = watches.face(&claims.sub, now());
                Envelope::Scalar(Scalar {
                    value: json!(reading(face.elapsed_ms)),
                    unit: None,
                    label: Some(if face.running { "Running" } else { "Stopped" }.to_string()),
                    previous: None,
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Stopwatches>> {
    Router::new()
        .route("/api/stopwatch", get(read))
        .route("/api/stopwatch/start", post(start))
        .route("/api/stopwatch/stop", post(stop))
        .route("/api/stopwatch/lap", post(lap))
        .route("/api/stopwatch/reset", post(reset))
}

async fn read(State(platform): State<Platform<Stopwatches>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|watches| watches.face(&claims.sub, now()))).into_response()
}

/// Every write is one rule on the asker's own stopwatch, answered with it.
fn act(
    platform: &Platform<Stopwatches>,
    write: &Write,
    rule: fn(&mut Stopwatches, &str, i64) -> Result<(), Refusal>,
) -> Response {
    platform.write(write, |watches, claims| {
        let at = now();
        rule(watches, &claims.sub, at)?;
        Ok(Reply::ok(watches.face(&claims.sub, at)))
    })
}

async fn start(State(platform): State<Platform<Stopwatches>>, write: Write) -> Response {
    act(&platform, &write, Stopwatches::start)
}

async fn stop(State(platform): State<Platform<Stopwatches>>, write: Write) -> Response {
    act(&platform, &write, Stopwatches::stop)
}

async fn lap(State(platform): State<Platform<Stopwatches>>, write: Write) -> Response {
    act(&platform, &write, Stopwatches::lap)
}

async fn reset(State(platform): State<Platform<Stopwatches>>, write: Write) -> Response {
    act(&platform, &write, |watches, who, _| watches.reset(who))
}
