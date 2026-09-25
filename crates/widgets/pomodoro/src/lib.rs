//! Widget 12 of twenty ([[HLIN-I-0012]]): a focus timer.
//!
//! Twenty-five minutes of focus, five of break, one person at a time. Not
//! shared: everybody has their own timer, and nobody else's panel changes
//! when they start theirs. Writes are still announced, as every widget's are,
//! so the same person's other browser shows the same countdown.
//!
//! The server keeps when a phase ends, not a ticking number: the module is
//! told how long is left and counts down with the browser's own clock, and a
//! read at any moment works out where the timer is from the time alone
//! ([`Timers::view`]).
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/pomodoro` | Your timer: the phase, time left, focuses finished |
//! | `POST` | `/api/pomodoro/start` | `{ "phase": "focus" }` or `{ "phase": "break" }` |
//! | `POST` | `/api/pomodoro/pause` | Pause a running phase |
//! | `POST` | `/api/pomodoro/resume` | Carry on a paused one |
//! | `DELETE` | `/api/pomodoro` | Stop, and do not count it |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the minutes left, as a `stat`.
//!
//! The rules:
//!
//! - One phase at a time: nothing starts while another is running or paused
//!   with time left.
//! - Only a running phase pauses, and only a paused one resumes, with the
//!   time it had left.
//! - A focus counts as finished once it has run its whole length, and is
//!   counted when the person moves on (starts the next phase, or stops).
//!   Stopping a focus early does not count it.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::{DateTime, Duration, Utc};
use hlin_widget_support::envelope::{Envelope, Scalar};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "pomodoro";

/// How long a focus is.
pub const FOCUS: Duration = Duration::minutes(25);

/// How long a break is.
pub const BREAK: Duration = Duration::minutes(5);

/// What a timer is timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Work.
    Focus,
    /// Rest.
    Break,
}

impl Phase {
    /// How long this phase lasts.
    pub fn length(self) -> Duration {
        match self {
            Self::Focus => FOCUS,
            Self::Break => BREAK,
        }
    }
}

/// Where a phase is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clock {
    /// Counting down to this instant.
    Running(DateTime<Utc>),
    /// Stopped with this much left.
    Paused(Duration),
}

/// One person's timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Timer {
    /// What it is timing, and where it is; nothing, when idle.
    current: Option<(Phase, Clock)>,
    /// Focuses run to the end.
    finished: u32,
}

impl Timer {
    fn left(&self, now: DateTime<Utc>) -> Option<Duration> {
        self.current.map(|(_, clock)| match clock {
            Clock::Running(ends) => (ends - now).max(Duration::zero()),
            Clock::Paused(left) => left,
        })
    }

    /// Whether the current phase has run its whole length.
    fn over(&self, now: DateTime<Utc>) -> bool {
        self.left(now) == Some(Duration::zero())
    }

    /// Clear the current phase, counting it if it was a focus that ran out.
    fn clear(&mut self, now: DateTime<Utc>) {
        if self.over(now) && matches!(self.current, Some((Phase::Focus, _))) {
            self.finished += 1;
        }
        self.current = None;
    }
}

/// Everybody's timers, by `sub`.
#[derive(Debug, Clone, Default)]
pub struct Timers {
    timers: BTreeMap<String, Timer>,
}

/// One person's timer, as they see it at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    /// What it is timing: `focus`, `break`, or nothing when idle.
    pub phase: Option<Phase>,
    /// Whether it is counting down now (not idle, paused or over).
    pub running: bool,
    /// Whether it is paused.
    pub paused: bool,
    /// Milliseconds left: `0` when over, and when idle.
    pub left_ms: i64,
    /// The phase's whole length, in milliseconds, for drawing how far it has
    /// gone.
    pub length_ms: i64,
    /// Focuses run to the end, counting one that has just run out.
    pub finished: u32,
}

/// What starting asks for.
#[derive(Debug, Deserialize)]
pub struct Start {
    /// Which phase.
    pub phase: Phase,
}

impl Timers {
    fn timer(&self, claims: &Claims) -> Timer {
        self.timers.get(&claims.sub).copied().unwrap_or_default()
    }

    /// `claims`'s timer at `now`.
    pub fn view(&self, claims: &Claims, now: DateTime<Utc>) -> View {
        let timer = self.timer(claims);
        let left = timer.left(now).unwrap_or_else(Duration::zero);
        let over = timer.over(now);
        let phase = timer.current.map(|(phase, _)| phase);
        View {
            phase,
            running: matches!(timer.current, Some((_, Clock::Running(_)))) && !over,
            paused: matches!(timer.current, Some((_, Clock::Paused(_)))),
            left_ms: left.num_milliseconds(),
            length_ms: phase.map_or(0, |phase| phase.length().num_milliseconds()),
            finished: timer.finished + u32::from(over && phase == Some(Phase::Focus)),
        }
    }

    /// Start `phase` for `claims` at `now`.
    pub fn start(
        &mut self,
        claims: &Claims,
        phase: Phase,
        now: DateTime<Utc>,
    ) -> Result<(), Refusal> {
        let mut timer = self.timer(claims);
        if timer.current.is_some() && !timer.over(now) {
            return Err(Refusal::conflict(match timer.current {
                Some((Phase::Focus, _)) => "A focus is already on; stop it first",
                _ => "A break is already on; stop it first",
            }));
        }
        timer.clear(now);
        timer.current = Some((phase, Clock::Running(now + phase.length())));
        self.timers.insert(claims.sub.clone(), timer);
        Ok(())
    }

    /// Pause `claims`'s running phase at `now`.
    pub fn pause(&mut self, claims: &Claims, now: DateTime<Utc>) -> Result<(), Refusal> {
        let mut timer = self.timer(claims);
        let Some((phase, Clock::Running(ends))) = timer.current else {
            return Err(Refusal::conflict("Nothing is running to pause"));
        };
        if timer.over(now) {
            return Err(Refusal::conflict("Time is already up"));
        }
        timer.current = Some((phase, Clock::Paused(ends - now)));
        self.timers.insert(claims.sub.clone(), timer);
        Ok(())
    }

    /// Carry on `claims`'s paused phase from `now`.
    pub fn resume(&mut self, claims: &Claims, now: DateTime<Utc>) -> Result<(), Refusal> {
        let mut timer = self.timer(claims);
        let Some((phase, Clock::Paused(left))) = timer.current else {
            return Err(Refusal::conflict("Nothing is paused to resume"));
        };
        timer.current = Some((phase, Clock::Running(now + left)));
        self.timers.insert(claims.sub.clone(), timer);
        Ok(())
    }

    /// Stop `claims`'s timer at `now`. A focus that ran out still counts.
    pub fn stop(&mut self, claims: &Claims, now: DateTime<Utc>) -> Result<(), Refusal> {
        let mut timer = self.timer(claims);
        if timer.current.is_none() {
            return Err(Refusal::conflict("The timer is not running"));
        }
        timer.clear(now);
        self.timers.insert(claims.sub.clone(), timer);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Timers> {
    Widget {
        panel: PANEL,
        name: "Pomodoro",
        icon: "timer",
        title: "Focus timer",
        description: "Twenty-five minutes of focus, five of break",
        shared: false,
        fallback: Some(Fallback {
            kind: "stat",
            envelope: "scalar.v1",
            // Counts down without anybody writing, so the shell has to ask
            // again; a minute is what the stat shows.
            refresh_ms: Some(60_000),
            data: |timers, claims| Envelope::Scalar(as_stat(&timers.view(claims, Utc::now()))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// Minutes left, rounded up, as a stat; nothing when idle.
fn as_stat(view: &View) -> Scalar {
    let minutes = (view.left_ms + 59_999) / 60_000;
    let label = match (view.phase, view.paused, view.left_ms) {
        (None, _, _) => "Not running".to_string(),
        (Some(_), _, 0) => "Time is up".to_string(),
        (Some(Phase::Focus), false, _) => "Focus".to_string(),
        (Some(Phase::Break), false, _) => "Break".to_string(),
        (Some(Phase::Focus), true, _) => "Focus, paused".to_string(),
        (Some(Phase::Break), true, _) => "Break, paused".to_string(),
    };
    Scalar {
        value: if view.phase.is_some() {
            minutes.into()
        } else {
            serde_json::Value::Null
        },
        unit: Some("min".to_string()),
        label: Some(label),
        previous: None,
        as_of: Some(Utc::now()),
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Timers>> {
    Router::new()
        .route("/api/pomodoro", get(read).delete(stop))
        .route("/api/pomodoro/start", post(start))
        .route("/api/pomodoro/pause", post(pause))
        .route("/api/pomodoro/resume", post(resume))
}

async fn read(State(platform): State<Platform<Timers>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|timers| timers.view(&claims, Utc::now()))).into_response()
}

async fn start(State(platform): State<Platform<Timers>>, write: Write) -> Response {
    platform.write(&write, |timers, claims| {
        let asked: Start =
            write.json("Starting is { \"phase\": \"focus\" } or { \"phase\": \"break\" }")?;
        let now = Utc::now();
        timers.start(claims, asked.phase, now)?;
        Ok(Reply::ok(timers.view(claims, now)))
    })
}

async fn pause(State(platform): State<Platform<Timers>>, write: Write) -> Response {
    platform.write(&write, |timers, claims| {
        let now = Utc::now();
        timers.pause(claims, now)?;
        Ok(Reply::ok(timers.view(claims, now)))
    })
}

async fn resume(State(platform): State<Platform<Timers>>, write: Write) -> Response {
    platform.write(&write, |timers, claims| {
        let now = Utc::now();
        timers.resume(claims, now)?;
        Ok(Reply::ok(timers.view(claims, now)))
    })
}

async fn stop(State(platform): State<Platform<Timers>>, write: Write) -> Response {
    platform.write(&write, |timers, claims| {
        let now = Utc::now();
        timers.stop(claims, now)?;
        Ok(Reply::ok(timers.view(claims, now)))
    })
}
