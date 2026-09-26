//! The focus timer, called the way the shell calls it, and its rules at
//! moments the test chooses (nobody waits twenty-five minutes for a test).

use chrono::{DateTime, Duration, TimeZone, Utc};
use hlin_widget_pomodoro::{BREAK, FOCUS, PANEL, Phase, Timers, api, widget};
use hlin_widget_support::Claims;
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{ModuleFiles, Site};
use serde_json::json;

async fn pomodoro() -> Running {
    testing::start(widget(), Timers::default(), api()).await
}

fn claims(sub: &str) -> Claims {
    Claims {
        iss: "hlin".to_string(),
        sub: sub.to_string(),
        aud: PANEL.to_string(),
        iat: 0,
        exp: 0,
        jti: "t".to_string(),
        name: None,
        email: None,
        groups: Vec::new(),
        extra: Default::default(),
    }
}

fn nine() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 25, 9, 0, 0).unwrap()
}

fn minutes(n: i64) -> Duration {
    Duration::minutes(n)
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_minutes_left_as_a_stat_nobody_else_is_told_of() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("stat"));
    assert_eq!(panel.envelope.as_deref(), Some("scalar.v1"));
    assert!(!panel.pushed, "a timer is its owner's");
}

#[test]
fn a_focus_counts_down_from_twenty_five_minutes() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    timers.start(&alice, Phase::Focus, nine()).unwrap();

    let view = timers.view(&alice, nine() + minutes(10));
    assert_eq!(view.phase, Some(Phase::Focus));
    assert!(view.running);
    assert_eq!(view.left_ms, minutes(15).num_milliseconds());
    assert_eq!(view.length_ms, FOCUS.num_milliseconds());
    assert_eq!(view.finished, 0);
}

#[test]
fn nothing_starts_while_a_phase_has_time_left() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    timers.start(&alice, Phase::Focus, nine()).unwrap();

    let refused = timers
        .start(&alice, Phase::Break, nine() + minutes(24))
        .unwrap_err();
    assert_eq!(refused.message, "A focus is already on; stop it first");

    timers.pause(&alice, nine() + minutes(5)).unwrap();
    assert!(
        timers
            .start(&alice, Phase::Focus, nine() + minutes(60))
            .is_err()
    );
}

#[test]
fn a_focus_that_runs_out_counts_once_the_break_starts() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    timers.start(&alice, Phase::Focus, nine()).unwrap();

    let up = timers.view(&alice, nine() + minutes(30));
    assert!(!up.running);
    assert_eq!(up.left_ms, 0);
    assert_eq!(up.finished, 1, "shown as finished the moment it runs out");

    timers
        .start(&alice, Phase::Break, nine() + minutes(30))
        .unwrap();
    let resting = timers.view(&alice, nine() + minutes(31));
    assert_eq!(resting.phase, Some(Phase::Break));
    assert_eq!(resting.left_ms, (BREAK - minutes(1)).num_milliseconds());
    assert_eq!(resting.finished, 1, "and counted once, not twice");
}

#[test]
fn a_focus_stopped_early_does_not_count_and_a_break_never_does() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    timers.start(&alice, Phase::Focus, nine()).unwrap();
    timers.stop(&alice, nine() + minutes(20)).unwrap();
    assert_eq!(timers.view(&alice, nine() + minutes(20)).finished, 0);

    timers
        .start(&alice, Phase::Break, nine() + minutes(20))
        .unwrap();
    timers.stop(&alice, nine() + minutes(40)).unwrap();
    let idle = timers.view(&alice, nine() + minutes(40));
    assert_eq!(idle.finished, 0);
    assert_eq!(idle.phase, None);
    assert_eq!(idle.left_ms, 0);
}

#[test]
fn a_pause_keeps_the_time_left_however_long_it_lasts() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    timers.start(&alice, Phase::Focus, nine()).unwrap();
    timers.pause(&alice, nine() + minutes(10)).unwrap();

    let paused = timers.view(&alice, nine() + minutes(90));
    assert!(paused.paused);
    assert!(!paused.running);
    assert_eq!(paused.left_ms, minutes(15).num_milliseconds());

    timers.resume(&alice, nine() + minutes(90)).unwrap();
    assert_eq!(
        timers.view(&alice, nine() + minutes(95)).left_ms,
        minutes(10).num_milliseconds()
    );
}

#[test]
fn only_a_running_phase_pauses_and_only_a_paused_one_resumes() {
    let mut timers = Timers::default();
    let alice = claims("u-alice");
    assert!(timers.pause(&alice, nine()).is_err());
    assert!(timers.resume(&alice, nine()).is_err());
    assert!(timers.stop(&alice, nine()).is_err());

    timers.start(&alice, Phase::Focus, nine()).unwrap();
    assert!(timers.resume(&alice, nine()).is_err());
    assert_eq!(
        timers
            .pause(&alice, nine() + minutes(26))
            .unwrap_err()
            .message,
        "Time is already up"
    );
}

#[test]
fn each_person_has_their_own_timer() {
    let mut timers = Timers::default();
    let (alice, bob) = (claims("u-alice"), claims("u-bob"));
    timers.start(&alice, Phase::Focus, nine()).unwrap();

    assert_eq!(timers.view(&bob, nine()).phase, None);
    timers.start(&bob, Phase::Break, nine()).unwrap();
    assert_eq!(timers.view(&alice, nine()).phase, Some(Phase::Focus));
}

#[tokio::test]
async fn starting_pausing_and_stopping_through_the_shell() {
    let pomodoro = pomodoro().await;
    let alice = person("u-alice", "Alice");

    let started = pomodoro
        .write(
            &alice,
            "POST",
            "/api/pomodoro/start",
            Some(json!({ "phase": "focus" })),
        )
        .await;
    assert_eq!(started.status, 200, "{}", started.body);
    assert_eq!(started.body["phase"], "focus");
    assert_eq!(started.body["running"], true);

    let again = pomodoro
        .write(
            &alice,
            "POST",
            "/api/pomodoro/start",
            Some(json!({ "phase": "break" })),
        )
        .await;
    assert_eq!(again.status, 409);
    assert_eq!(
        again.body,
        json!({ "message": "A focus is already on; stop it first" })
    );

    let paused = pomodoro
        .write(&alice, "POST", "/api/pomodoro/pause", None)
        .await;
    assert_eq!(paused.body["paused"], true);

    let stopped = pomodoro
        .write(&alice, "DELETE", "/api/pomodoro", None)
        .await;
    assert_eq!(stopped.status, 200);
    assert_eq!(
        pomodoro.get(&alice, "/api/pomodoro").await.body["phase"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn a_phase_must_be_focus_or_break() {
    let pomodoro = pomodoro().await;
    let alice = person("u-alice", "Alice");
    let refused = pomodoro
        .write(
            &alice,
            "POST",
            "/api/pomodoro/start",
            Some(json!({ "phase": "nap" })),
        )
        .await;
    assert_eq!(refused.status, 400);
}

#[tokio::test]
async fn a_start_is_announced_on_the_event_stream() {
    let pomodoro = pomodoro().await;
    let alice = person("u-alice", "Alice");
    let mut events = pomodoro.listen(&alice).await;

    pomodoro
        .write(
            &alice,
            "POST",
            "/api/pomodoro/start",
            Some(json!({ "phase": "focus" })),
        )
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_says_the_minutes_left() {
    let pomodoro = pomodoro().await;
    let alice = person("u-alice", "Alice");

    let Envelope::Scalar(idle) = pomodoro.fallback(&alice).await else {
        panic!("the timer's fallback is a scalar");
    };
    assert_eq!(idle.value, serde_json::Value::Null);
    assert_eq!(idle.label.as_deref(), Some("Not running"));

    pomodoro
        .write(
            &alice,
            "POST",
            "/api/pomodoro/start",
            Some(json!({ "phase": "break" })),
        )
        .await;
    let Envelope::Scalar(resting) = pomodoro.fallback(&alice).await else {
        panic!("the timer's fallback is a scalar");
    };
    assert_eq!(resting.value, 5);
    assert_eq!(resting.label.as_deref(), Some("Break"));
}

// -- Its own UI (HLIN-I-0013) ---------------------------------------------

/// The focus timer as its own UI reaches it: `/api/` from the root, as the demo's
/// local user, over the same handlers as Hlin's `/hlin/api/`.
async fn pomodoro_with_its_own_ui() -> Running {
    let layout = Site {
        local_user: Some("Local User".to_string()),
        ..Site::default()
    };
    testing::start_site(
        widget(),
        Timers::default(),
        api(),
        ModuleFiles::none(),
        layout,
    )
    .await
}

#[tokio::test]
async fn its_own_ui_starts_a_focus_and_reads_it_back() {
    let pomodoro = pomodoro_with_its_own_ui().await;
    let started = pomodoro
        .own(
            "POST",
            "/api/pomodoro/start",
            Some(&testing::fresh_key()),
            Some(json!({ "phase": "focus" })),
        )
        .await;
    assert_eq!(started.status, 200, "{}", started.body);

    let timer = pomodoro.own("GET", "/api/pomodoro", None, None).await;
    assert_eq!(timer.body["phase"], "focus");
}
