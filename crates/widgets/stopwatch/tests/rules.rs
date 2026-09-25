//! The stopwatch: its rules at chosen instants, and the widget called the way
//! the shell calls it.

use hlin_widget_stopwatch::{MOST_LAPS, PANEL, Stopwatches, api, reading, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

async fn stopwatch() -> Running {
    testing::start(widget(), Stopwatches::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_stat_and_the_stopwatch_is_nobody_elses() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("stat"));
    assert!(!panel.pushed, "starting yours is news to nobody else");
    assert_eq!(panel.refresh_ms, Some(5_000), "it moves without a write");
}

#[test]
fn it_counts_while_running_and_carries_on_after_a_stop() {
    let mut watches = Stopwatches::default();
    watches.start("alice", 1_000).unwrap();
    assert_eq!(watches.face("alice", 3_500).elapsed_ms, 2_500);
    watches.stop("alice", 4_000).unwrap();
    assert_eq!(
        watches.face("alice", 60_000).elapsed_ms,
        3_000,
        "stopped is stopped"
    );
    watches.start("alice", 100_000).unwrap();
    assert_eq!(watches.face("alice", 100_250).elapsed_ms, 3_250);
    assert!(watches.face("alice", 100_250).running);
}

#[test]
fn laps_say_when_and_how_long_each_took() {
    let mut watches = Stopwatches::default();
    watches.start("alice", 0).unwrap();
    watches.lap("alice", 1_000).unwrap();
    watches.lap("alice", 2_500).unwrap();
    let laps = watches.face("alice", 3_000).laps;
    assert_eq!(
        laps.iter()
            .map(|lap| (lap.number, lap.at_ms, lap.split_ms))
            .collect::<Vec<_>>(),
        [(1, 1_000, 1_000), (2, 2_500, 1_500)]
    );
}

#[test]
fn start_stop_lap_and_reset_only_when_they_make_sense() {
    let mut watches = Stopwatches::default();
    assert_eq!(
        watches.stop("alice", 0).unwrap_err().message,
        "Your stopwatch is not running"
    );
    assert_eq!(
        watches.lap("alice", 0).unwrap_err().message,
        "Start the stopwatch to mark a lap"
    );
    watches.start("alice", 0).unwrap();
    assert_eq!(
        watches.start("alice", 1).unwrap_err().message,
        "Your stopwatch is already running"
    );
    assert_eq!(
        watches.reset("alice").unwrap_err().message,
        "Stop the stopwatch before resetting it"
    );
    watches.stop("alice", 10).unwrap();
    watches.reset("alice").unwrap();
    assert_eq!(watches.face("alice", 99).elapsed_ms, 0);
}

#[test]
fn twenty_laps_is_the_most() {
    let mut watches = Stopwatches::default();
    watches.start("alice", 0).unwrap();
    for at in 0..MOST_LAPS as i64 {
        watches.lap("alice", at).unwrap();
    }
    let refused = watches.lap("alice", 99).unwrap_err();
    assert_eq!(refused.status, 409);
    assert_eq!(refused.message, "20 laps is the most; reset to start again");
}

#[test]
fn a_stopwatch_reads_as_minutes_seconds_and_tenths() {
    assert_eq!(reading(0), "0:00.0");
    assert_eq!(reading(62_345), "1:02.3");
    assert_eq!(reading(3_723_400), "1:02:03.4");
}

#[tokio::test]
async fn everybody_has_their_own() {
    let stopwatch = stopwatch().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let started = stopwatch
        .write(&alice, "POST", "/api/stopwatch/start", None)
        .await;
    assert_eq!(started.status, 200, "{}", started.body);
    assert_eq!(started.body["running"], true);

    let bobs = stopwatch.get(&bob, "/api/stopwatch").await;
    assert_eq!(bobs.body["running"], false);
    assert_eq!(bobs.body["elapsed_ms"], 0);
    let refused = stopwatch
        .write(&bob, "POST", "/api/stopwatch/stop", None)
        .await;
    assert_eq!(refused.status, 409, "Alice's running is not Bob's");
    assert_eq!(
        stopwatch.get(&alice, "/api/stopwatch").await.body["running"],
        true
    );
}

#[tokio::test]
async fn a_write_is_announced_so_the_owners_other_browser_follows() {
    let stopwatch = stopwatch().await;
    let alice = person("u-alice", "Alice");
    let mut events = stopwatch.listen(&alice).await;
    stopwatch
        .write(&alice, "POST", "/api/stopwatch/start", None)
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Scalar(stat) = stopwatch.fallback(&alice).await else {
        panic!("the stopwatch's fallback is a scalar");
    };
    assert_eq!(stat.label.as_deref(), Some("Running"));
    assert!(stat.value.as_str().unwrap().starts_with("0:0"));
}
