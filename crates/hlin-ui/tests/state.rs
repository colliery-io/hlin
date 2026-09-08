//! What the browser believes, and when it stops believing it.
//!
//! These run on the host, with no browser, because the parts worth being sure
//! about have no DOM in them: applying a frame, ignoring one that answers a
//! question nobody is asking any more, and deciding that a silent shell has
//! gone rather than merely paused.

use chrono::{Duration, TimeZone, Utc};
use hlin_stream::{Frame, PanelFrame, SurfaceFrame};
use hlin_ui::SurfaceState;
use hlin_view::{Cause, PanelState};

fn at(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_757_244_600 + seconds, 0).unwrap()
}

fn envelope() -> hlin_manifest::Envelope {
    hlin_manifest::parse_envelope(br#"{ "envelope": "scalar.v1", "value": 42 }"#, "scalar.v1")
        .expect("fixture")
}

fn ready(instance: &str, generation: u64) -> Frame {
    let mut frame = PanelFrame::new(instance, generation, PanelState::Ready);
    frame.envelope = Some(envelope());
    frame.age_seconds = Some(0);
    Frame::Panel(Box::new(frame))
}

fn unavailable(instance: &str, generation: u64, cause: Cause) -> Frame {
    let mut frame = PanelFrame::new(instance, generation, PanelState::Unavailable(cause));
    frame.detail = Some("a reason from the shell".to_string());
    Frame::Panel(Box::new(frame))
}

// -- Taking frames --------------------------------------------------------

#[test]
fn a_frame_becomes_a_panel() {
    let mut state = SurfaceState::new();
    assert!(state.apply(&ready("p1", 1)));

    let panel = state.panel("p1").expect("the panel exists");
    assert_eq!(panel.state, PanelState::Ready);
    assert!(panel.envelope.is_some());
}

#[test]
fn a_later_frame_replaces_an_earlier_one() {
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));
    state.apply(&unavailable("p1", 1, Cause::Unreachable));

    let panel = state.panel("p1").unwrap();
    assert_eq!(panel.state, PanelState::Unavailable(Cause::Unreachable));
    assert_eq!(
        state.panels().len(),
        1,
        "it is the same panel, not a new one"
    );
}

#[test]
fn a_frame_answering_an_old_question_is_dropped() {
    // The reason generations exist on both sides. A slow answer to a previous
    // time range must not paint over the current one.
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 5));

    assert!(!state.apply(&unavailable("p1", 3, Cause::Malformed)));
    assert_eq!(
        state.panel("p1").unwrap().state,
        PanelState::Ready,
        "the panel keeps what the current generation told it"
    );
}

#[test]
fn asking_for_new_parameters_moves_the_generation_forward_at_once() {
    // Locally, before the shell has acknowledged anything, so a frame
    // answering the previous question is recognisable as stale immediately.
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));

    let next = state.next_generation();
    assert_eq!(next, 2);

    assert!(
        !state.apply(&ready("p1", 1)),
        "the old answer is already stale"
    );
}

#[test]
fn an_acknowledgement_moves_the_generation_but_not_backwards() {
    let mut state = SurfaceState::new();

    state.apply(&Frame::Surface(SurfaceFrame::acknowledging(4)));
    assert_eq!(state.generation(), 4);

    assert!(!state.apply(&Frame::Surface(SurfaceFrame::acknowledging(2))));
    assert_eq!(state.generation(), 4);
}

#[test]
fn the_viewers_own_choices_survive_a_frame() {
    // A frame says what the platform is doing; the kind a person picked is
    // theirs, and the shell has no business overwriting it.
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));
    state.set_kind("p1", Some("sparkline".to_string()));

    state.apply(&ready("p1", 1));

    assert_eq!(
        state.panel("p1").unwrap().kind_override.as_deref(),
        Some("sparkline")
    );
}

// -- Losing the shell -----------------------------------------------------

#[test]
fn losing_the_stream_makes_everything_stale_at_once() {
    // The only state the browser derives for itself, because the shell cannot
    // report its own absence.
    let mut state = SurfaceState::new();
    state.apply(&ready("a", 1));
    state.apply(&ready("b", 1));

    state.on_stream_lost(at(0));

    assert!(state.is_disconnected());
    for panel in state.panels() {
        assert_eq!(panel.state, PanelState::Stale);
        assert!(panel.envelope.is_some(), "stale still shows what it had");
    }
}

#[test]
fn a_silence_that_goes_on_becomes_an_unreachable_shell() {
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));
    state.on_stream_lost(at(0));

    state.on_tick(at(10), Duration::seconds(30));
    assert_eq!(
        state.panel("p1").unwrap().state,
        PanelState::Stale,
        "inside the grace interval it is merely not current"
    );

    state.on_tick(at(31), Duration::seconds(30));
    let panel = state.panel("p1").unwrap();
    assert_eq!(panel.state, PanelState::Unavailable(Cause::Unreachable));
    assert!(
        panel.detail.as_ref().unwrap().contains("Hlin"),
        "and the viewer is told it is the shell that is gone, not a platform"
    );
}

#[test]
fn nothing_ages_while_the_stream_is_healthy() {
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));

    state.on_tick(at(600), Duration::seconds(30));

    assert_eq!(
        state.panel("p1").unwrap().state,
        PanelState::Ready,
        "staleness of data is the shell's judgement, not the browser's"
    );
}

#[test]
fn an_already_unavailable_panel_is_not_relabelled_by_a_lost_stream() {
    let mut state = SurfaceState::new();
    state.apply(&unavailable("p1", 1, Cause::Forbidden));

    state.on_stream_lost(at(0));
    state.on_tick(at(60), Duration::seconds(30));

    assert_eq!(
        state.panel("p1").unwrap().state,
        PanelState::Unavailable(Cause::Forbidden),
        "a viewer who lacks access still lacks it when the shell goes away"
    );
}

#[test]
fn reconnecting_waits_to_be_told_rather_than_guessing() {
    let mut state = SurfaceState::new();
    state.apply(&ready("p1", 1));
    state.on_stream_lost(at(0));
    state.on_tick(at(60), Duration::seconds(30));

    state.on_stream_restored();
    assert!(!state.is_disconnected());

    // Nothing is repaired by reconnecting itself; the shell resends current
    // state on subscribe, and that is what puts the panel right.
    assert_eq!(
        state.panel("p1").unwrap().state,
        PanelState::Unavailable(Cause::Unreachable)
    );

    state.apply(&ready("p1", 1));
    assert_eq!(state.panel("p1").unwrap().state, PanelState::Ready);
}

#[test]
fn the_picker_says_pending_until_the_shell_acknowledges() {
    let mut state = SurfaceState::new();
    assert!(
        state.applied(),
        "a surface nobody has touched is showing what was asked for"
    );

    let asked = state.next_generation();
    assert!(
        !state.applied(),
        "the moment a control moves, what is on screen answers the old question"
    );

    state.apply(&Frame::Surface(SurfaceFrame::acknowledging(asked)));
    assert!(state.applied(), "and the shell saying so is what clears it");
}

#[test]
fn a_frame_that_is_not_an_acknowledgement_does_not_pretend_to_be_one() {
    let mut state = SurfaceState::new();
    let asked = state.next_generation();

    state.apply(&Frame::Surface(SurfaceFrame {
        protocol_version: hlin_stream::PROTOCOL_VERSION,
        generation: asked,
        acknowledged: false,
    }));

    assert!(
        !state.applied(),
        "only an acknowledgement acknowledges, or a browser could report a \
         parameter change as applied that the shell never received"
    );
}

#[test]
fn a_stream_that_never_opened_is_given_up_on_after_the_grace_interval() {
    // A panel on the layout that the stream has never mentioned is not in this
    // state at all, so `on_tick` cannot speak for it. The surface asks this
    // instead, and must not answer yes while there is still reason to wait.
    let mut state = SurfaceState::new();
    let grace = Duration::seconds(30);

    assert!(
        !state.given_up(at(0), grace),
        "a stream that has not been lost is not one to give up on"
    );

    state.on_stream_lost(at(0));
    assert!(
        !state.given_up(at(29), grace),
        "inside the grace interval it may still come back"
    );
    assert!(
        state.given_up(at(30), grace),
        "and past it, nothing is coming"
    );

    state.on_stream_restored();
    assert!(
        !state.given_up(at(120), grace),
        "and a stream that came back is not given up on, however long it was away"
    );
}
