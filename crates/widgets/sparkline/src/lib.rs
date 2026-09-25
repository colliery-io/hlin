//! Widget 8 of twenty ([[HLIN-I-0012]]): a synthetic metric over the time
//! range.
//!
//! The one widget of the twenty that follows the surface's time picker. Its
//! fallback is a series, so its panel declares `time_range`
//! (`hlin_widget_support::Fallback`): the shell shows the picker, sends the
//! range to the module in `context`, and the module asks for that range.
//!
//! Not shared. The metric is made up, and the same for everybody: a function
//! of the minute, a daily swell, a faster ripple and some jitter, so the demo
//! shows the same shape every time and a test can say what it will be. Which
//! of the metrics you watch is yours; choosing is announced, as every
//! widget's writes are, so your other browser follows.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/sparkline?from_ms=&to_ms=` | Your metric over the range, in epoch milliseconds |
//! | `PUT` | `/api/sparkline/metric` | `{ "metric": id }`: watch another |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: your metric over the last day at one point a minute, as a
//! `timeseries`, which the support crate cuts to the range the shell asks for.
//!
//! The rules:
//!
//! - A range starts before it ends and is at most [`LONGEST_DAYS`] days. With
//!   no range, the last hour.
//! - Nothing from the future: a range reaching past now stops at now.
//! - At most [`POINTS`] points, on steps of whole minutes aligned to the
//!   epoch, so the line does not shimmer as the range slides.
//! - Only the metrics there are: [`METRICS`].

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use hlin_widget_support::envelope::{Envelope, Line, Point, Series};
use hlin_widget_support::{Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "sparkline";

/// The most points one answer carries: plenty for a line a panel wide.
pub const POINTS: i64 = 60;

/// The longest range, in days.
pub const LONGEST_DAYS: i64 = 31;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// Every metric: id, what it is called, its unit, and its shape.
pub const METRICS: &[Metric] = &[
    Metric {
        id: "requests",
        label: "Requests",
        unit: "req/s",
        base: 420.0,
        swell: 180.0,
        ripple: 35.0,
        jitter: 20.0,
        salt: 1,
    },
    Metric {
        id: "latency",
        label: "p95 latency",
        unit: "ms",
        base: 180.0,
        swell: 45.0,
        ripple: 25.0,
        jitter: 12.0,
        salt: 2,
    },
    Metric {
        id: "errors",
        label: "Error rate",
        unit: "%",
        base: 1.2,
        swell: 0.5,
        ripple: 0.4,
        jitter: 0.3,
        salt: 3,
    },
];

/// One made-up metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metric {
    /// Its id.
    pub id: &'static str,
    /// What it is called.
    pub label: &'static str,
    /// Its unit.
    pub unit: &'static str,
    base: f64,
    /// How far the daily swell carries it.
    swell: f64,
    /// How far the forty-minute ripple carries it.
    ripple: f64,
    /// How far a minute's jitter carries it.
    jitter: f64,
    salt: u64,
}

impl Metric {
    /// Its value in the minute holding `at`, in epoch milliseconds. Never
    /// below zero, and to two decimal places.
    pub fn at(&self, at: i64) -> f64 {
        let minute = at.div_euclid(MINUTE);
        let day = std::f64::consts::TAU * (minute.rem_euclid(1440) as f64) / 1440.0;
        let ripple = std::f64::consts::TAU * (minute.rem_euclid(40) as f64) / 40.0;
        let value = self.base - self.swell * day.cos()
            + self.ripple * ripple.sin()
            + self.jitter * noise(minute as u64 ^ (self.salt << 56));
        (value.max(0.0) * 100.0).round() / 100.0
    }
}

/// A number in `-1.0..1.0` that depends only on `seed`.
fn noise(seed: u64) -> f64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

fn metric(id: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|metric| metric.id == id)
}

/// The time now, in epoch milliseconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Who watches which metric, by `sub`. Nobody is in here until they choose;
/// until then, [`METRICS`]' first.
#[derive(Debug, Clone, Default)]
pub struct Sparklines {
    watching: BTreeMap<String, &'static str>,
}

/// A range, as the module asks for it.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Asked {
    /// Start, in epoch milliseconds.
    pub from_ms: Option<i64>,
    /// End, in epoch milliseconds.
    pub to_ms: Option<i64>,
}

/// A metric over a range, as one person sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drawn {
    /// Which metric.
    pub metric: &'static str,
    /// What it is called.
    pub label: &'static str,
    /// Its unit.
    pub unit: &'static str,
    /// The range answered, after the rules: `[from, to)`, epoch milliseconds.
    pub from_ms: i64,
    /// See `from_ms`.
    pub to_ms: i64,
    /// The step between points, in milliseconds.
    pub step_ms: i64,
    /// `[at, value]`, oldest first.
    pub points: Vec<(i64, f64)>,
    /// The metrics there are, `[id, label]`, to choose from.
    pub metrics: Vec<(&'static str, &'static str)>,
}

/// What choosing a metric asks for.
#[derive(Debug, Deserialize)]
pub struct Choose {
    /// Its id.
    pub metric: String,
}

impl Sparklines {
    /// Which metric `who` watches.
    pub fn watching(&self, who: &str) -> &'static Metric {
        self.watching
            .get(who)
            .and_then(|id| metric(id))
            .unwrap_or(&METRICS[0])
    }

    /// Watch another metric.
    pub fn choose(&mut self, who: &str, id: &str) -> Result<(), Refusal> {
        let Some(metric) = metric(id) else {
            return Err(Refusal::not_found("There is no such metric"));
        };
        self.watching.insert(who.to_string(), metric.id);
        Ok(())
    }

    /// `who`'s metric over `asked`, as it stands at `now`.
    pub fn drawn(&self, who: &str, asked: Asked, now: i64) -> Result<Drawn, Refusal> {
        let to = asked.to_ms.unwrap_or(now).min(now);
        let from = asked.from_ms.unwrap_or(to - HOUR);
        if let (Some(from), Some(to)) = (asked.from_ms, asked.to_ms)
            && from >= to
        {
            return Err(Refusal::bad_request("The range ends before it starts"));
        }
        if to - from > LONGEST_DAYS * DAY {
            return Err(Refusal::bad_request(format!(
                "A range is at most {LONGEST_DAYS} days"
            )));
        }
        let step = step_for(to - from);
        let first =
            from.div_euclid(step) * step + if from.rem_euclid(step) == 0 { 0 } else { step };
        let metric = self.watching(who);
        Ok(Drawn {
            metric: metric.id,
            label: metric.label,
            unit: metric.unit,
            from_ms: from,
            to_ms: to,
            step_ms: step,
            points: (0..)
                .map(|n| first + n * step)
                .take_while(|at| *at < to)
                .map(|at| (at, metric.at(at)))
                .collect(),
            metrics: METRICS
                .iter()
                .map(|metric| (metric.id, metric.label))
                .collect(),
        })
    }
}

/// The step for a span: whole minutes, and no more than [`POINTS`] of them.
pub fn step_for(span: i64) -> i64 {
    let minutes = (span.max(1) + POINTS * MINUTE - 1) / (POINTS * MINUTE);
    minutes.max(1) * MINUTE
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Sparklines> {
    Widget {
        panel: PANEL,
        name: "Sparkline",
        icon: "activity",
        title: "Sparkline",
        description: "A made-up metric over the surface's time range",
        shared: false,
        fallback: Some(Fallback {
            kind: "timeseries",
            envelope: "series.v1",
            refresh_ms: Some(60_000),
            data: |sparklines, claims| {
                let metric = sparklines.watching(&claims.sub);
                let to = now().div_euclid(MINUTE) * MINUTE;
                Envelope::Series(Series {
                    series: vec![Line {
                        name: metric.label.to_string(),
                        points: (0..=1440)
                            .map(|minutes_ago| to - (1440 - minutes_ago) * MINUTE)
                            .map(|at| Point(at, Some(metric.at(at))))
                            .collect(),
                        extra: Default::default(),
                    }],
                    unit: Some(metric.unit.to_string()),
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Sparklines>> {
    Router::new()
        .route("/api/sparkline", get(read))
        .route("/api/sparkline/metric", put(choose))
}

async fn read(
    State(platform): State<Platform<Sparklines>>,
    Viewer(claims): Viewer,
    asked: Result<Query<Asked>, QueryRejection>,
) -> Response {
    let Ok(Query(asked)) = asked else {
        return Refusal::bad_request("A range is from_ms and to_ms, in epoch milliseconds")
            .into_response();
    };
    match platform.read(|sparklines| sparklines.drawn(&claims.sub, asked, now())) {
        Ok(drawn) => axum::Json(drawn).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

async fn choose(State(platform): State<Platform<Sparklines>>, write: Write) -> Response {
    platform.write(&write, |sparklines, claims| {
        let asked: Choose = write.json("Choosing is { \"metric\": id }")?;
        sparklines.choose(&claims.sub, &asked.metric)?;
        Ok(Reply::ok(sparklines.watching(&claims.sub).id))
    })
}
