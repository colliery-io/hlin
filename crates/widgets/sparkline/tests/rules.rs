//! The sparkline: its ranges and points at chosen instants, and the widget
//! called the way the shell calls it.

use hlin_widget_sparkline::{
    Asked, LONGEST_DAYS, METRICS, PANEL, POINTS, Sparklines, api, step_for, widget,
};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{ModuleFiles, Site};
use serde_json::json;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
/// 2026-01-15T12:00:00Z: in the past, so the rules' `now` can be later.
const NOON: i64 = 1_768_478_400_000;

fn asked(from: i64, to: i64) -> Asked {
    Asked {
        from_ms: Some(from),
        to_ms: Some(to),
    }
}

async fn sparkline() -> Running {
    testing::start(widget(), Sparklines::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn it_declares_the_time_range_and_falls_back_to_a_timeseries() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("timeseries"));
    assert_eq!(panel.envelope.as_deref(), Some("series.v1"));
    assert_eq!(
        panel
            .params
            .iter()
            .map(|param| param.param.as_str())
            .collect::<Vec<_>>(),
        ["time_range"],
        "the shell shows its picker, and sends the module the range"
    );
    assert!(!panel.pushed);
}

#[test]
fn the_metric_is_the_same_for_everybody_every_time_and_never_negative() {
    for metric in METRICS {
        for minute in 0..3 * 1440 {
            let at = NOON + minute * MINUTE;
            let value = metric.at(at);
            assert!(value >= 0.0, "{} at {at}", metric.id);
            assert_eq!(value, metric.at(at + 59_999), "one value a minute");
        }
    }
    let one = Sparklines::default().drawn("alice", asked(NOON - HOUR, NOON), NOON);
    let other = Sparklines::default().drawn("bob", asked(NOON - HOUR, NOON), NOON);
    assert_eq!(one.unwrap().points, other.unwrap().points);
}

#[test]
fn an_hour_is_sixty_points_a_minute_apart_and_a_day_is_sixty_too() {
    let sparklines = Sparklines::default();
    let hour = sparklines
        .drawn("alice", asked(NOON - HOUR, NOON), NOON)
        .unwrap();
    assert_eq!(hour.points.len(), 60);
    assert_eq!(hour.step_ms, MINUTE);
    assert_eq!(hour.points[0].0, NOON - HOUR);
    assert!(
        hour.points.iter().all(|(at, _)| *at < NOON),
        "to is exclusive"
    );

    let day = sparklines
        .drawn("alice", asked(NOON - DAY, NOON), NOON)
        .unwrap();
    assert_eq!(day.points.len() as i64, POINTS);
    assert_eq!(day.step_ms, 24 * MINUTE);
}

#[test]
fn steps_are_whole_minutes_aligned_so_a_sliding_range_does_not_shimmer() {
    assert_eq!(step_for(15 * MINUTE), MINUTE);
    assert_eq!(step_for(6 * HOUR), 6 * MINUTE);
    assert_eq!(step_for(7 * DAY), 168 * MINUTE);
    let sparklines = Sparklines::default();
    let then = sparklines
        .drawn(
            "alice",
            asked(NOON - 6 * HOUR + 7_000, NOON + 7_000),
            NOON + 7_000,
        )
        .unwrap();
    // Six minutes on, and the line has moved one step and no more.
    let on = 6 * MINUTE;
    let later = sparklines
        .drawn(
            "alice",
            asked(NOON - 6 * HOUR + 7_000 + on, NOON + 7_000 + on),
            NOON + 7_000 + on,
        )
        .unwrap();
    assert!(then.points.iter().all(|(at, _)| at % (6 * MINUTE) == 0));
    assert_eq!(then.points[1..], later.points[..then.points.len() - 1]);
}

#[test]
fn nothing_from_the_future_and_with_no_range_the_last_hour() {
    let sparklines = Sparklines::default();
    let ahead = sparklines
        .drawn("alice", asked(NOON - HOUR, NOON + HOUR), NOON)
        .unwrap();
    assert_eq!(ahead.to_ms, NOON);
    assert!(ahead.points.iter().all(|(at, _)| *at < NOON));

    let unasked = sparklines.drawn("alice", Asked::default(), NOON).unwrap();
    assert_eq!((unasked.from_ms, unasked.to_ms), (NOON - HOUR, NOON));
}

#[test]
fn a_range_starts_before_it_ends_and_is_at_most_a_month() {
    let sparklines = Sparklines::default();
    assert_eq!(
        sparklines
            .drawn("alice", asked(NOON, NOON - 1), NOON)
            .unwrap_err()
            .message,
        "The range ends before it starts"
    );
    let too_long = sparklines
        .drawn("alice", asked(NOON - (LONGEST_DAYS * DAY + 1), NOON), NOON)
        .unwrap_err();
    assert_eq!(too_long.status, 400);
    assert_eq!(too_long.message, "A range is at most 31 days");
}

#[test]
fn which_metric_is_each_persons_own() {
    let mut sparklines = Sparklines::default();
    assert_eq!(sparklines.watching("alice").id, "requests");
    sparklines.choose("alice", "latency").unwrap();
    assert_eq!(sparklines.watching("alice").id, "latency");
    assert_eq!(sparklines.watching("bob").id, "requests");
    assert_eq!(
        sparklines.choose("alice", "vibes").unwrap_err().message,
        "There is no such metric"
    );
}

#[tokio::test]
async fn the_module_asks_for_the_range_in_milliseconds() {
    let sparkline = sparkline().await;
    let alice = person("u-alice", "Alice");
    let drawn = sparkline
        .get(
            &alice,
            &format!("/api/sparkline?from_ms={}&to_ms={}", NOON - HOUR, NOON),
        )
        .await;
    assert_eq!(drawn.status, 200, "{}", drawn.body);
    assert_eq!(drawn.body["points"].as_array().unwrap().len(), 60);
    assert_eq!(drawn.body["unit"], "req/s");

    let refused = sparkline
        .get(&alice, "/api/sparkline?from_ms=yesterday")
        .await;
    assert_eq!(refused.status, 400);
}

#[tokio::test]
async fn choosing_is_announced_and_the_fallback_follows_it() {
    let sparkline = sparkline().await;
    let alice = person("u-alice", "Alice");
    let mut events = sparkline.listen(&alice).await;
    let chosen = sparkline
        .write(
            &alice,
            "PUT",
            "/api/sparkline/metric",
            Some(json!({ "metric": "errors" })),
        )
        .await;
    assert_eq!(chosen.body, json!("errors"));
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Series(series) = sparkline.fallback(&alice).await else {
        panic!("the sparkline's fallback is a series");
    };
    assert_eq!(series.unit.as_deref(), Some("%"));
    assert_eq!(series.series[0].name, "Error rate");
    assert_eq!(
        series.series[0].points.len(),
        1441,
        "a day, a point a minute"
    );
}

#[tokio::test]
async fn the_fallback_is_cut_to_the_range_the_shell_asks_for() {
    let sparkline = sparkline().await;
    let alice = person("u-alice", "Alice");
    let to = chrono::Utc::now();
    let from = to - chrono::Duration::minutes(15);
    let query = format!(
        "from={}&to={}&step=60",
        from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    let Envelope::Series(series) = sparkline.fallback_asking(&alice, &query).await else {
        panic!("the sparkline's fallback is a series");
    };
    let points = series.series[0].points.len();
    assert!((14..=16).contains(&points), "{points} points in 15 minutes");
}

// -- Its own UI (HLIN-I-0013) ---------------------------------------------

/// The sparkline as its own UI reaches it: `/api/` from the root, as the demo's
/// local user, over the same handlers as Hlin's `/hlin/api/`.
async fn sparkline_with_its_own_ui() -> Running {
    let layout = Site {
        local_user: Some("Local User".to_string()),
        ..Site::default()
    };
    testing::start_site(
        widget(),
        Sparklines::default(),
        api(),
        ModuleFiles::none(),
        layout,
    )
    .await
}

#[tokio::test]
async fn its_own_ui_chooses_a_metric_and_reads_it_back() {
    let sparkline = sparkline_with_its_own_ui().await;
    let chosen = sparkline
        .own(
            "PUT",
            "/api/sparkline/metric",
            Some(&testing::fresh_key()),
            Some(json!({ "metric": "errors" })),
        )
        .await;
    assert_eq!(chosen.status, 200, "{}", chosen.body);

    let drawn = sparkline.own("GET", "/api/sparkline", None, None).await;
    assert_eq!(drawn.body["metric"], "errors");
}
