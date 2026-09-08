//! Synthetic data that looks like a signal.
//!
//! Everything here is deterministic in `(platform, endpoint, window)`, so the
//! demo shows the same shapes on every run and the tests can assert on values.
//! It is not random: a chart of noise tells a viewer nothing about whether the
//! chart works, and the demo's whole job is to be looked at.

use chrono::{DateTime, Duration, Utc};
use hlin_manifest::envelope::{
    Choice, Column, ColumnType, Envelope, Health, Line, Options, Point, Records, Scalar, Series,
    Status, StatusItem,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The window and resolution a request asked for.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// Start of the range.
    pub from: DateTime<Utc>,
    /// End of the range.
    pub to: DateTime<Utc>,
    /// Requested resolution in seconds, as the shell's `step` hint.
    pub step_seconds: i64,
}

impl Window {
    /// The window a request implies, with defaults for anything absent.
    ///
    /// A platform that ignored `step` would be within its rights; this one
    /// honours it, because the demo is partly about showing that the hint
    /// reaches a platform and does something.
    pub fn new(
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        step_seconds: Option<i64>,
    ) -> Self {
        let to = to.unwrap_or_else(Utc::now);
        let from = from.unwrap_or_else(|| to - Duration::hours(1));
        let span = (to - from).num_seconds().max(1);
        let step = step_seconds.unwrap_or(60).clamp(1, span);
        Self {
            from,
            to,
            step_seconds: step,
        }
    }

    /// The instants a series should carry, at roughly one per `step`.
    fn instants(&self) -> Vec<i64> {
        let start = self.from.timestamp_millis();
        let end = self.to.timestamp_millis();
        let step = self.step_seconds * 1000;
        let mut points = Vec::new();
        let mut at = start;
        while at <= end && points.len() < 5_000 {
            points.push(at);
            at += step;
        }
        points
    }
}

/// A small deterministic hash, so the same inputs always give the same shape.
fn seed(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// A smooth, bounded wave with a seeded phase, so different series look
/// different but each is stable across runs.
///
/// `period_seconds` is the thing to get right, and it is easy to get wrong in a
/// way nothing catches. A quantity that in reality moves second to second, given
/// a four-minute period here, is a panel that redraws at whatever cadence it
/// declares and looks completely static while it does it — the shell working
/// perfectly and the demo showing nothing. Measured on the composed surface
/// before this was fixed: six of its eight panels rendered once in six seconds.
///
/// So the period belongs to the quantity, not to the chart. An ingest rate and a
/// queue depth move in seconds; a rollup over a chosen hour, and a list of which
/// stage reads from which, do not.
fn wave(seed: u64, at_millis: i64, period_seconds: f64, amplitude: f64, centre: f64) -> f64 {
    let phase = (seed % 1000) as f64 / 1000.0 * std::f64::consts::TAU;
    let t = at_millis as f64 / 1000.0 / period_seconds * std::f64::consts::TAU;
    // Two waves of different periods, so the line has some character without
    // being noise.
    let value =
        centre + amplitude * (t + phase).sin() + amplitude * 0.35 * (t * 2.7 + phase * 1.4).sin();
    value.max(0.0)
}

/// The workers this platform reports, named from its own id.
fn workers(platform: &str) -> Vec<String> {
    ["a", "b", "c"]
        .iter()
        .map(|suffix| format!("{platform}-worker-{suffix}"))
        .collect()
}

/// The clusters this platform's `select` offers.
pub fn clusters(platform: &str) -> Vec<(String, String)> {
    vec![
        (format!("{platform}-us-east"), "US East".to_string()),
        (format!("{platform}-eu-west"), "EU West".to_string()),
        (format!("{platform}-lab"), "Lab".to_string()),
    ]
}

/// `scalar.v1`: the current rate.
pub fn records_per_second(platform: &str) -> Envelope {
    let now = Utc::now();
    let base = seed(&[platform, "records-per-second"]);
    let value = wave(base, now.timestamp_millis(), 7.0, 400.0, 1500.0);
    let previous = wave(base, now.timestamp_millis() - 300_000, 7.0, 400.0, 1500.0);

    Envelope::Scalar(Scalar {
        value: Value::from(value.round()),
        unit: Some("per_second".to_string()),
        label: Some("records/s".to_string()),
        previous: Some(previous.round()),
        as_of: Some(now),
        extra: Default::default(),
    })
}

/// `series.v1`: one line per worker.
pub fn throughput(platform: &str, window: Window) -> Envelope {
    let series = workers(platform)
        .into_iter()
        .map(|name| line(&name, platform, window, None))
        .collect();

    Envelope::Series(Series {
        series,
        unit: Some("per_second".to_string()),
        as_of: Some(Utc::now()),
        extra: Default::default(),
    })
}

/// `series.v1`, narrowed to one cluster by the `select` parameter.
///
/// With no selection the platform applies its own default, which the parameter
/// vocabulary says is the platform's business ([[HLIN-S-0002]]).
pub fn throughput_by_cluster(platform: &str, cluster: Option<&str>) -> Envelope {
    let available = clusters(platform);
    let chosen = cluster
        .filter(|selected| available.iter().any(|(value, _)| value == selected))
        .map(str::to_string)
        .unwrap_or_else(|| available[0].0.clone());

    // A rolling live window rather than the viewer's chosen range, and the
    // control is the reason. This is the panel somebody is meant to poke at —
    // pick a cluster, watch the answer change — and over an hour at
    // sixty-second steps it could not visibly change at all: sixty of its
    // sixty-one points redraw in the same place and only the right edge moves,
    // once a minute. A chart nobody can see responding is a poor advertisement
    // for a control that works.
    //
    // `throughput` still answers the time picker over an hour, which is the
    // honest slow case and worth keeping somewhere.
    let now = Utc::now();
    let end = now.timestamp_millis();
    let start = end - LIVE_WINDOW_SECONDS * 1000;
    let step = 250;

    let series = workers(platform)
        .into_iter()
        .take(2)
        .map(|name| {
            let base = seed(&[platform, &name, &chosen]);
            let mut points = Vec::new();
            let mut at = start;
            while at <= end {
                // Each cluster its own phase and centre, so switching one
                // visibly changes the shape rather than the label.
                let centre = 420.0 + (base % 5) as f64 * 90.0;
                let value = wave(base, at, 9.0, 140.0, centre);
                points.push(Point(at, Some((value * 10.0).round() / 10.0)));
                at += step;
            }
            Line {
                name: name.clone(),
                points,
                extra: Default::default(),
            }
        })
        .collect();

    Envelope::Series(Series {
        series,
        unit: Some("per_second".to_string()),
        as_of: Some(now),
        extra: Default::default(),
    })
}

fn line(name: &str, platform: &str, window: Window, cluster: Option<&str>) -> Line {
    let base = seed(&[platform, name, cluster.unwrap_or("")]);
    let instants = window.instants();

    // One deliberate gap per series, so a viewer can see that a gap is drawn as
    // a break rather than as a drop to zero.
    let gap_at = instants.len() / 3;

    let points = instants
        .into_iter()
        .enumerate()
        .map(|(index, at)| {
            if index == gap_at {
                Point(at, None)
            } else {
                let value = wave(base, at, 1800.0, 25.0, 60.0);
                Point(at, Some((value * 10.0).round() / 10.0))
            }
        })
        .collect();

    Line {
        name: name.to_string(),
        points,
        extra: Default::default(),
    }
}

// -- Live data ------------------------------------------------------------
//
// The panels above move on the scale a metrics dashboard usually does: their
// waves have periods of fifteen to thirty minutes, so a viewer watching for a
// few seconds sees a number that is barely changing. That is the right shape
// for what they represent and the wrong shape for showing that the shell is
// live, which is why these are separate panels rather than a faster setting on
// those.
//
// Periods here are in seconds, so the signal is visibly in motion at a glance
// and a refresh that failed to arrive is obvious rather than inferred. Still
// deterministic in wall-clock time, for the reason at the top of this file.

/// How long the live series looks back. Short, because a minute at quarter-
/// second resolution is already 240 points, and because the point of the panel
/// is the leading edge rather than the history.
const LIVE_WINDOW_SECONDS: i64 = 60;

/// `scalar.v1`, moving fast enough to watch.
///
/// `previous` is the value one second ago rather than five minutes ago, so the
/// delta a pack draws beside the number is a delta over a period a person can
/// perceive.
pub fn live_rate(platform: &str) -> Envelope {
    let now = Utc::now();
    let base = seed(&[platform, "live-rate"]);
    let at = now.timestamp_millis();
    let value = wave(base, at, 11.0, 180.0, 640.0);
    let previous = wave(base, at - 1_000, 11.0, 180.0, 640.0);

    Envelope::Scalar(Scalar {
        value: Value::from(value.round()),
        unit: Some("per_second".to_string()),
        label: Some("events/s".to_string()),
        previous: Some(previous.round()),
        as_of: Some(now),
        extra: Default::default(),
    })
}

/// `series.v1`: a rolling window that ends now, whatever was asked for.
///
/// This ignores the `time_range` a surface sends, which is a liberty the
/// parameter vocabulary permits — honouring a hint is a platform's choice
/// ([[HLIN-S-0002]]) — and the honest one here. A live panel that answered a
/// window chosen an hour ago would draw an hour-old chart at eight frames a
/// second, which is worse than not being live at all.
pub fn live_throughput(platform: &str) -> Envelope {
    let now = Utc::now();
    let end = now.timestamp_millis();
    let start = end - LIVE_WINDOW_SECONDS * 1000;
    let step = 250;

    let series = ["ingest", "commit"]
        .iter()
        .map(|name| {
            let base = seed(&[platform, "live-throughput", name]);
            let mut points = Vec::new();
            let mut at = start;
            while at <= end {
                let value = wave(base, at, 11.0, 180.0, 640.0);
                points.push(Point(at, Some((value * 10.0).round() / 10.0)));
                at += step;
            }
            Line {
                name: format!("{platform}-{name}"),
                points,
                extra: Default::default(),
            }
        })
        .collect();

    Envelope::Series(Series {
        series,
        unit: Some("per_second".to_string()),
        as_of: Some(now),
        extra: Default::default(),
    })
}

// -- Panels that ask for a component by name ------------------------------
//
// These two declare a `component` in the manifest as well as a `kind`. The
// component is a name this platform and a design system agreed on; Hlin
// forwards it without reading it. A shell whose pack does not offer that
// component draws the `kind` instead, which is why both of these are useful
// panels in their own right and not placeholders waiting for a design system.

/// `scalar.v1`, drawn as `aurora.meter` where that exists and a stat elsewhere.
///
/// The `max` in `extra` is the other half of the same agreement: `scalar.v1`
/// has no field for a maximum, and a meter needs one. An envelope's `extra` is
/// exactly where a convention like that belongs, because it travels with the
/// data and costs Hlin's contract nothing.
pub fn saturation(platform: &str) -> Envelope {
    let now = Utc::now();
    let base = seed(&[platform, "saturation"]);
    let value = wave(base, now.timestamp_millis(), 19.0, 30.0, 62.0).min(100.0);

    let mut extra = BTreeMap::new();
    extra.insert("max".to_string(), Value::from(100.0));

    Envelope::Scalar(Scalar {
        value: Value::from(value.round()),
        unit: Some("percent".to_string()),
        label: Some("of capacity".to_string()),
        previous: Some(wave(base, now.timestamp_millis() - 300_000, 19.0, 30.0, 62.0).round()),
        as_of: Some(now),
        extra,
    })
}

/// `records.v1`: the pipeline, as rows that also describe a graph.
///
/// One envelope, two readings. A pack with no graph component draws the table
/// this plainly is — stage, role, state, what it reads from — which is a
/// perfectly good panel. A pack that offers `aurora.graph` reads the same rows
/// as nodes and edges and draws the DAG.
///
/// Worth being clear about what is happening: Hlin has no vocabulary for a
/// graph and gains none here. `records.v1` is unchanged, the acceptance matrix
/// is unchanged, and the shell still believes this is a table. The graph exists
/// entirely in an agreement between this platform and a design system.
pub fn pipeline(health: &[crate::changes::Health]) -> Envelope {
    let now = Utc::now();

    // `depends_on` is what a table shows as a column and a graph reads as an
    // edge. The same field, doing both jobs, is the whole trick.
    let stages: [(&str, &str, &str, Option<&str>); 5] = [
        ("ingest", "Ingest", "source", None),
        ("parse", "Parse", "transform", Some("ingest")),
        ("enrich", "Enrich", "transform", Some("parse")),
        ("index", "Index", "sink", Some("enrich")),
        ("archive", "Archive", "sink", Some("enrich")),
    ];

    let rows = stages
        .iter()
        .enumerate()
        .map(|(index, (id, label, role, depends_on))| {
            // Read, not derived. A pipeline's shape is fixed — which stage
            // reads from which does not change while you watch — but how the
            // stages *are* is real state a background task changes, which is
            // what lets this platform say when it changed rather than leaving
            // the shell to keep asking.
            let state = health
                .get(index)
                .copied()
                .unwrap_or(crate::changes::Health::Healthy)
                .word();

            let mut row = Map::new();
            row.insert("id".to_string(), Value::String((*id).to_string()));
            row.insert("label".to_string(), Value::String((*label).to_string()));
            row.insert("role".to_string(), Value::String((*role).to_string()));
            row.insert("state".to_string(), Value::String(state.to_string()));
            row.insert(
                "depends_on".to_string(),
                match depends_on {
                    Some(from) => Value::String((*from).to_string()),
                    None => Value::Null,
                },
            );
            row
        })
        .collect();

    Envelope::Records(Records {
        columns: vec![
            column("label", "Stage", ColumnType::String, None),
            column("role", "Role", ColumnType::String, None),
            column("state", "State", ColumnType::String, None),
            column("depends_on", "Reads from", ColumnType::String, None),
        ],
        rows,
        as_of: Some(now),
        extra: Default::default(),
    })
}

/// `records.v1`: work waiting, by stage.
pub fn queue_depth(platform: &str) -> Envelope {
    let now = Utc::now();
    let stages = ["parse", "enrich", "index", "publish"];

    let rows = stages
        .iter()
        .map(|stage| {
            let base = seed(&[platform, "queue-depth", stage]);
            let depth = wave(base, now.timestamp_millis(), 13.0, 30.0, 35.0).round();
            let oldest = now - Duration::seconds((base % 600) as i64);

            let mut row = Map::new();
            row.insert("stage".to_string(), Value::String((*stage).to_string()));
            row.insert("depth".to_string(), Value::from(depth));
            row.insert("oldest".to_string(), Value::String(oldest.to_rfc3339()));
            row
        })
        .collect();

    Envelope::Records(Records {
        columns: vec![
            column("stage", "Stage", ColumnType::String, None),
            column("depth", "Depth", ColumnType::Number, Some("count")),
            column("oldest", "Oldest", ColumnType::Timestamp, None),
        ],
        rows,
        as_of: Some(now),
        extra: Default::default(),
    })
}

fn column(key: &str, label: &str, value_type: ColumnType, unit: Option<&str>) -> Column {
    Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: unit.map(str::to_string),
        extra: Default::default(),
    }
}

/// `status.v1`: the rollup and its parts.
pub fn worker_health(platform: &str) -> Envelope {
    let now = Utc::now();
    let items: Vec<StatusItem> = workers(platform)
        .into_iter()
        .map(|name| {
            let base = seed(&[platform, "health", &name]);
            // Stable per worker, so the demo shows a consistent story rather
            // than flickering between states.
            let (status, detail) = match base % 5 {
                0 => (Health::Degraded, Some("restarting".to_string())),
                _ => (Health::Ok, None),
            };
            StatusItem {
                name,
                status,
                detail,
                extra: Default::default(),
            }
        })
        .collect();

    let degraded = items
        .iter()
        .filter(|item| item.status != Health::Ok)
        .count();
    let rollup = if degraded == 0 {
        Health::Ok
    } else if degraded < items.len() {
        Health::Degraded
    } else {
        Health::Down
    };

    Envelope::Status(Status {
        status: rollup,
        label: Some("Ingest".to_string()),
        detail: (degraded > 0).then(|| format!("{degraded} of {} workers restarting", items.len())),
        since: Some(now - Duration::minutes(28)),
        items,
        as_of: Some(now),
        extra: Default::default(),
    })
}

/// `options.v1`: what the `select` parameter offers.
pub fn cluster_options(platform: &str) -> Envelope {
    let options = clusters(platform)
        .into_iter()
        .map(|(value, label)| Choice {
            value,
            label,
            group: Some("Production".to_string()),
            extra: Default::default(),
        })
        .collect();

    Envelope::Options(Options {
        options,
        as_of: Some(Utc::now()),
        extra: Default::default(),
    })
}

/// `scalar.v1`: a count this platform actually keeps, rather than derives.
///
/// The one endpoint here whose answer is not a function of the clock. It reads
/// state a background task advances, which is what makes it the panel worth
/// pushing: nothing about `now()` predicts when it moves.
pub fn batches(completed: u64) -> Envelope {
    Envelope::Scalar(Scalar {
        value: Value::from(completed),
        unit: Some("batches".to_string()),
        label: Some("completed".to_string()),
        // No `previous`: a count that only goes up says nothing useful by
        // comparing itself to a moment ago, and a delta that is always
        // non-negative is a worse chart than the number itself.
        previous: None,
        as_of: Some(Utc::now()),
        extra: Default::default(),
    })
}
