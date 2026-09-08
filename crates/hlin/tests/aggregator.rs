//! The panel state machine, against a clock the test controls.
//!
//! Every row of the outcome table in HLIN-S-0003, plus the promises around it:
//! two panels asking for one thing produce one request, a dragged picker
//! produces one fan-out, a slow answer to an old time range never paints over
//! a new one, and a platform that is down is asked once per interval rather
//! than once per panel.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use hlin::stream::frame::{Frame, WireCause, WireState};
use hlin::stream::{Instance, Outcome, Policy, Request, Surface, TimeRange};
use hlin_manifest::Envelope;
use hlin_view::{Cause, PanelState};

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_757_244_600 + seconds, 0).unwrap()
}

fn envelope() -> Envelope {
    hlin_manifest::parse_envelope(
        br#"{ "envelope": "scalar.v1", "value": 42, "unit": "count" }"#,
        "scalar.v1",
    )
    .expect("fixture")
}

fn instance(id: &str, platform: &str, endpoint: &str) -> Instance {
    Instance::new(id, platform, "a-panel", endpoint, "scalar.v1")
}

fn surface(instances: Vec<Instance>) -> Surface {
    Surface::new(instances, Policy::default())
}

/// The panel frames in a batch, for readable assertions.
fn panels(frames: &[Frame]) -> Vec<&hlin::stream::PanelFrame> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Panel(panel) => Some(panel.as_ref()),
            Frame::Surface(_) => None,
        })
        .collect()
}

fn state_of(surface: &Surface, id: &str) -> PanelState {
    surface.instance(id).expect("instance exists").state()
}

// -- The outcome table ----------------------------------------------------

#[test]
fn a_good_answer_makes_a_panel_ready() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));

    let frames = surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    assert_eq!(state_of(&surface, "p1"), PanelState::Ready);
    let frame = panels(&frames)[0];
    assert_eq!(frame.state, WireState::Ready);
    assert!(frame.envelope.is_some(), "a ready panel carries its data");
}

#[test]
fn silence_makes_a_panel_unreachable_and_it_keeps_what_it_had() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    let (requests, _) = surface.due(at(60));
    let frames = surface.resolve(&requests[0], Outcome::Unreachable, at(60));

    assert_eq!(
        state_of(&surface, "p1"),
        PanelState::Unavailable(Cause::Unreachable)
    );

    let frame = panels(&frames)[0];
    assert_eq!(frame.cause, Some(WireCause::Unreachable));
    assert!(
        frame.envelope.is_some(),
        "during an incident, aged data beats an empty box"
    );
}

#[test]
fn a_refusal_makes_a_panel_forbidden_and_it_shows_nothing() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    let (requests, _) = surface.due(at(60));
    let frames = surface.resolve(&requests[0], Outcome::Forbidden, at(60));

    let frame = panels(&frames)[0];
    assert_eq!(frame.cause, Some(WireCause::Forbidden));
    assert!(
        frame.envelope.is_none(),
        "data this viewer may not see must not be left on screen"
    );
    assert!(frame.detail.as_ref().unwrap().contains("access"));
}

#[test]
fn something_unusable_makes_a_panel_malformed_and_it_shows_nothing() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    let (requests, _) = surface.due(at(60));
    let frames = surface.resolve(&requests[0], Outcome::Malformed, at(60));

    let frame = panels(&frames)[0];
    assert_eq!(frame.cause, Some(WireCause::Malformed));
    assert!(
        frame.envelope.is_none(),
        "data known to be wrong is not shown"
    );
}

#[test]
fn the_status_mapping_matches_the_specification() {
    assert_eq!(Outcome::from_failure(403), Outcome::Forbidden);
    // The shell's own credential was refused: an operator's problem, not the
    // viewer's, so it is not `forbidden`.
    assert_eq!(Outcome::from_failure(401), Outcome::Malformed);
    // The manifest says the panel exists, so a 404 means the platform and its
    // own manifest disagree.
    assert_eq!(Outcome::from_failure(404), Outcome::Malformed);
    assert_eq!(Outcome::from_failure(422), Outcome::Malformed);
    assert_eq!(Outcome::from_failure(500), Outcome::Unreachable);
    assert_eq!(Outcome::from_failure(503), Outcome::Unreachable);
}

#[test]
fn the_registry_can_retire_a_panel_without_a_fetch() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    let frame = surface
        .set_registry_state("p1", Cause::Unknown, at(10))
        .expect("a frame");

    let Frame::Panel(frame) = frame else {
        panic!("expected a panel frame")
    };
    assert_eq!(frame.cause, Some(WireCause::Unknown));
    assert!(frame.envelope.is_none());
}

#[test]
fn a_panel_goes_stale_once_its_data_has_aged() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    assert!(surface.tick(at(60)).is_empty(), "still fresh at a minute");

    let frames = surface.tick(at(120));
    assert_eq!(state_of(&surface, "p1"), PanelState::Stale);

    let frame = panels(&frames)[0];
    assert_eq!(frame.state, WireState::Stale);
    assert!(frame.envelope.is_some(), "stale still shows its data");
    assert!(frame.age_seconds.unwrap() >= 120);
}

// -- Fan-out ---------------------------------------------------------------

#[test]
fn two_panels_asking_for_one_thing_produce_one_request() {
    // The vision's promise, made concrete. Two panels on one endpoint must not
    // become two requests.
    let mut surface = surface(vec![
        instance("chart", "orebank", "api/throughput"),
        instance("spark", "orebank", "api/throughput"),
    ]);

    let (requests, _) = surface.due(at(0));

    assert_eq!(requests.len(), 1, "one endpoint, one request");
    assert_eq!(requests[0].instances.len(), 2, "and two panels waiting");

    let frames = surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));
    assert_eq!(frames.len(), 2, "one answer, two frames");
    assert_eq!(state_of(&surface, "chart"), PanelState::Ready);
    assert_eq!(state_of(&surface, "spark"), PanelState::Ready);
}

#[test]
fn panels_on_different_endpoints_are_fetched_separately() {
    let mut surface = surface(vec![
        instance("a", "orebank", "api/one"),
        instance("b", "orebank", "api/two"),
        instance("c", "stampmill", "api/one"),
    ]);

    let (requests, _) = surface.due(at(0));
    assert_eq!(
        requests.len(),
        3,
        "a different platform is a different request"
    );
}

#[test]
fn instances_with_different_selections_are_fetched_separately() {
    let mut surface = surface(vec![
        accepting("a", "orebank", "api/x", &["cluster"]),
        accepting("b", "orebank", "api/x", &["cluster"]),
    ]);

    let mut selections = BTreeMap::new();
    selections.insert(
        "a".to_string(),
        BTreeMap::from([("cluster".to_string(), vec!["us-east".to_string()])]),
    );
    selections.insert(
        "b".to_string(),
        BTreeMap::from([("cluster".to_string(), vec!["eu-west".to_string()])]),
    );

    surface.set_params(2, None, selections, at(0));
    let (requests, _) = surface.due(at(1));

    assert_eq!(
        requests.len(),
        2,
        "two panels asking different questions are two requests"
    );
}

// -- Generations and coalescing -------------------------------------------

#[test]
fn a_dragged_picker_produces_one_fan_out() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let range = TimeRange {
        from: at(-3600),
        to: at(0),
    };

    // Five changes inside the settle interval.
    for generation in 2..=6 {
        surface.set_params(generation, Some(range), BTreeMap::new(), at(0));
        let (requests, _) = surface.due(at(0));
        assert!(
            requests.is_empty(),
            "nothing is fetched while a change is still settling"
        );
    }

    // Once it settles, one round of requests for the latest generation.
    let (requests, _) = surface.due(at(1));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].generation, 6, "for the latest parameters only");
}

#[test]
fn a_parameter_change_is_acknowledged_before_any_panel_answers() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let frame = surface
        .set_params(2, None, BTreeMap::new(), at(0))
        .expect("an acknowledgement");

    let Frame::Surface(frame) = frame else {
        panic!("expected a surface frame")
    };
    assert_eq!(frame.generation, 2);
    assert!(frame.acknowledged);
}

#[test]
fn a_slow_answer_to_an_old_time_range_never_paints_over_a_new_one() {
    // The reason generations exist. A request from before a picker moved must
    // not deliver its answer afterwards.
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (old_requests, _) = surface.due(at(0));

    surface.set_params(2, None, BTreeMap::new(), at(1));

    let frames = surface.resolve(
        &old_requests[0],
        Outcome::Ready(Box::new(envelope())),
        at(2),
    );

    assert!(
        frames.is_empty(),
        "an answer from a superseded generation is dropped"
    );
    assert_eq!(
        state_of(&surface, "p1"),
        PanelState::Loading,
        "and the panel is still waiting for the new one"
    );
}

#[test]
fn an_out_of_order_parameter_message_cannot_rewind_a_surface() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    surface.set_params(5, None, BTreeMap::new(), at(0));
    assert_eq!(surface.generation(), 5);

    let ignored = surface.set_params(3, None, BTreeMap::new(), at(1));
    assert!(ignored.is_none(), "an older generation is not acknowledged");
    assert_eq!(surface.generation(), 5);
}

#[test]
fn a_generation_change_refetches_even_data_that_is_still_fresh() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    // Nothing is due yet on time alone.
    let (requests, _) = surface.due(at(5));
    assert!(requests.is_empty());

    // But a new time range means the data on screen answers the wrong question.
    surface.set_params(2, None, BTreeMap::new(), at(5));
    let (requests, _) = surface.due(at(6));
    assert_eq!(requests.len(), 1);
}

// -- Backoff ---------------------------------------------------------------

#[test]
fn a_platform_that_is_down_is_asked_once_per_interval_not_once_per_panel() {
    // Backoff is per platform. Otherwise a viewer with six panels from one
    // failing platform generates six times the load, and every viewer
    // multiplies it again.
    let mut surface = surface(vec![
        instance("a", "orebank", "api/one"),
        instance("b", "orebank", "api/two"),
        instance("c", "orebank", "api/three"),
    ]);

    let (requests, _) = surface.due(at(0));
    assert_eq!(requests.len(), 3);

    for request in &requests {
        surface.resolve(request, Outcome::Unreachable, at(0));
    }

    let (requests, _) = surface.due(at(1));
    assert!(
        requests.is_empty(),
        "the whole platform waits, not each panel separately"
    );
}

#[test]
fn backoff_grows_and_then_stops_growing() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let policy = Policy::default();

    let mut now = 0;
    let mut waits = Vec::new();

    for _ in 0..8 {
        let (requests, _) = surface.due(at(now));
        if requests.is_empty() {
            now += 1;
            continue;
        }
        let started = now;
        surface.resolve(&requests[0], Outcome::Unreachable, at(now));

        // Walk forward until it is willing to try again.
        loop {
            now += 1;
            let (requests, _) = surface.due(at(now));
            if !requests.is_empty() {
                waits.push(now - started);
                break;
            }
            if now - started > policy.retry_ceiling.num_seconds() * 2 {
                panic!("backoff never expired");
            }
        }
    }

    assert!(waits[1] > waits[0], "the wait grows after repeated failure");
    assert!(
        waits
            .iter()
            .all(|wait| *wait <= policy.retry_ceiling.num_seconds() + 1),
        "and stops growing at the ceiling: {waits:?}"
    );
}

#[test]
fn a_platform_that_answers_at_all_is_forgiven_immediately() {
    // Even a refusal proves the platform is there; only silence earns a wait.
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Unreachable, at(0));

    let (requests, _) = surface.due(at(31));
    assert_eq!(requests.len(), 1, "it is tried again after the first wait");
    surface.resolve(&requests[0], Outcome::Forbidden, at(31));

    let (requests, _) = surface.due(at(62));
    assert_eq!(
        requests.len(),
        1,
        "and having answered, it is back on the ordinary refresh"
    );
}

// -- What a browser is told on arrival ------------------------------------

#[test]
fn a_subscriber_is_told_the_current_state_of_everything() {
    // A reconnecting browser must be correct without the shell replaying
    // history (HLIN-S-0003 NFR-1.1).
    let mut surface = surface(vec![
        instance("a", "orebank", "api/one"),
        instance("b", "stampmill", "api/two"),
    ]);

    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(0));
    }
    surface.set_registry_state("b", Cause::Deprecated, at(1));

    let frames = surface.current_frames(at(2));
    let frames = panels(&frames);

    assert_eq!(frames.len(), 2, "every panel, whatever state it is in");
    assert_eq!(frames[0].state, WireState::Ready);
    assert_eq!(frames[1].cause, Some(WireCause::Deprecated));
}

#[test]
fn a_panel_with_nothing_to_show_says_it_is_loading() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (_, frames) = surface.due(at(0));

    assert_eq!(panels(&frames)[0].state, WireState::Loading);
}

#[test]
fn a_panel_holding_data_keeps_showing_it_while_the_next_fetch_is_in_flight() {
    // A chart that blanked on every refresh would be unusable.
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    let (_, frames) = surface.due(at(60));
    assert!(
        frames.is_empty(),
        "no loading frame for a panel that already has something to show"
    );
    assert_eq!(state_of(&surface, "p1"), PanelState::Ready);
}

// -- What a viewer is told -------------------------------------------------

#[test]
fn nothing_a_platform_wrote_reaches_a_viewer() {
    // The detail is written by the shell from the cause. A platform's error
    // body has no path to a viewer's screen.
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));

    for outcome in [Outcome::Unreachable, Outcome::Malformed, Outcome::Forbidden] {
        let frames = surface.resolve(&requests[0], outcome, at(0));
        let detail = panels(&frames)[0].detail.clone().expect("a reason");
        assert!(!detail.is_empty());
        assert!(!detail.contains("api/x"), "not even the endpoint");
    }
}

#[test]
fn a_retired_panel_offers_its_successor_and_others_do_not() {
    let mut retiring = instance("p1", "orebank", "api/x");
    retiring.successor = Some("the-new-one".to_string());
    let mut surface = surface(vec![retiring]);

    let frame = surface
        .set_registry_state("p1", Cause::Deprecated, at(0))
        .expect("a frame");
    let Frame::Panel(frame) = frame else {
        panic!("expected a panel frame")
    };
    assert_eq!(frame.successor.as_deref(), Some("the-new-one"));

    let frame = surface
        .set_registry_state("p1", Cause::Forbidden, at(1))
        .expect("a frame");
    let Frame::Panel(frame) = frame else {
        panic!("expected a panel frame")
    };
    assert!(
        frame.successor.is_none(),
        "a successor is only useful where one might exist"
    );
}

// -- The wire format -------------------------------------------------------

#[test]
fn frames_round_trip_through_json() {
    let mut surface = surface(vec![instance("p1", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    let frames = surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    for frame in &frames {
        let json = frame.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["protocol_version"], hlin::stream::PROTOCOL_VERSION);
        assert_eq!(parsed["instance"], "p1");
        assert_eq!(parsed["state"], "ready");
        assert_eq!(frame.event(), "panel");
    }
}

#[test]
fn the_specifications_example_frame_parses() {
    let example = r#"{
      "protocol_version": 1,
      "instance": "panel-3",
      "generation": 7,
      "state": "ready",
      "as_of": "2026-09-07T11:30:00Z",
      "age_seconds": 4,
      "envelope": { "envelope": "scalar.v1", "value": 1523, "unit": "per_second" }
    }"#;

    let frame: hlin::stream::PanelFrame = serde_json::from_str(example).expect("parses");
    assert_eq!(frame.instance, "panel-3");
    assert_eq!(frame.generation, 7);
    assert_eq!(frame.panel_state(), PanelState::Ready);
    assert!(frame.envelope.is_some());
}

#[test]
fn the_specifications_example_params_request_parses() {
    let example = r#"{ "protocol_version": 1, "generation": 7,
      "time_range": { "from": "2026-09-07T10:00:00Z", "to": "2026-09-07T11:00:00Z" },
      "selections": { "panel-3": { "cluster": ["us-east"] } } }"#;

    let request: hlin::stream::ParamsRequest = serde_json::from_str(example).expect("parses");
    assert_eq!(request.generation, 7);
    assert!(request.time_range.is_some());
    assert_eq!(request.selections["panel-3"]["cluster"], vec!["us-east"]);
}

#[test]
fn a_step_hint_falls_out_of_the_window_and_the_width() {
    let hour = TimeRange {
        from: at(0),
        to: at(3600),
    };
    let day = TimeRange {
        from: at(0),
        to: at(86_400),
    };

    assert!(hour.step_seconds(600) < day.step_seconds(600));
    assert!(hour.step_seconds(600) >= 1);
}

#[test]
fn requests_for_the_same_thing_share_a_key() {
    let one = Request {
        platform_id: "orebank".to_string(),
        endpoint: "api/x".to_string(),
        query: "from=a&to=b".to_string(),
        generation: 1,
        instances: vec!["a".to_string()],
    };
    let other = Request {
        instances: vec!["b".to_string()],
        ..one.clone()
    };

    assert_eq!(
        one.key(),
        other.key(),
        "the waiting panels are not part of it"
    );
}

#[test]
fn a_panel_its_platform_no_longer_offers_is_shown_and_never_asked_for() {
    // A layout outlives the panels on it, so a surface can name one the
    // registry has stopped accepting. It still occupies its place and still
    // draws, saying what happened to it.
    let mut surface = surface(vec![
        instance("live", "orebank", "api/throughput"),
        Instance::retired("gone", "orebank", "panel-that-was-removed"),
    ]);

    assert_eq!(
        state_of(&surface, "gone"),
        PanelState::Unavailable(Cause::Unknown),
        "it is born unavailable rather than pretending to load"
    );

    let (requests, _) = surface.due(at(0));

    assert_eq!(
        requests.len(),
        1,
        "only the live panel is asked for; a retired one has no endpoint, and \
         asking anyway would turn 'no longer offered' into whatever the \
         platform happens to answer at a URL it never promised"
    );
    assert_eq!(requests[0].instances, vec!["live".to_string()]);
}

#[test]
fn a_layouts_stored_choices_are_in_force_before_anyone_touches_a_control() {
    let mut surface = surface(vec![accepting(
        "a",
        "orebank",
        "api/by-cluster",
        &["cluster"],
    )]);

    let mut chosen = BTreeMap::new();
    chosen.insert("cluster".to_string(), vec!["orebank-lab".to_string()]);
    let mut selections = BTreeMap::new();
    selections.insert("a".to_string(), chosen);
    surface.restore_selections(selections);

    let (requests, _) = surface.due(at(0));

    assert_eq!(
        requests[0].query, "cluster=orebank-lab",
        "otherwise someone who chose a cluster and reloaded would get the \
         platform's default with nothing on screen saying why"
    );
}

#[test]
fn a_panel_the_registry_withdraws_stops_being_asked_for() {
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);
    surface.resolve(
        &Request {
            platform_id: "orebank".to_string(),
            endpoint: "api/throughput".to_string(),
            query: String::new(),
            generation: 1,
            instances: vec!["a".to_string()],
        },
        Outcome::Ready(Box::new(envelope())),
        at(0),
    );
    assert_eq!(state_of(&surface, "a"), PanelState::Ready);

    surface.set_registry_state("a", Cause::Unknown, at(1));

    let (requests, _) = surface.due(at(120));
    assert!(
        requests.is_empty(),
        "asking a platform for a panel its manifest no longer declares would \
         turn 'no longer offered' into a 404, and the 404 into 'malformed'"
    );
}

#[test]
fn saying_a_panel_is_gone_twice_sends_one_frame() {
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);

    assert!(
        surface
            .set_registry_state("a", Cause::Unknown, at(0))
            .is_some(),
        "the first time is news"
    );
    assert!(
        surface
            .set_registry_state("a", Cause::Unknown, at(1))
            .is_none(),
        "the second is not, and a check that runs every second must not send a \
         frame every second"
    );
}

#[test]
fn a_panel_offered_again_goes_back_to_loading_rather_than_to_its_old_data() {
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);
    surface.resolve(
        &Request {
            platform_id: "orebank".to_string(),
            endpoint: "api/throughput".to_string(),
            query: String::new(),
            generation: 1,
            instances: vec!["a".to_string()],
        },
        Outcome::Ready(Box::new(envelope())),
        at(0),
    );
    surface.set_registry_state("a", Cause::Unknown, at(1));

    let frame = surface.clear_registry_state("a", at(2));

    assert!(frame.is_some());
    assert_eq!(
        state_of(&surface, "a"),
        PanelState::Loading,
        "a panel that has been away has no current data, and showing the number \
         it had before the platform changed its mind would be a lie"
    );

    let (requests, _) = surface.due(at(3));
    assert_eq!(requests.len(), 1, "and it is asked for again");
}

#[test]
fn clearing_a_panel_that_was_never_withdrawn_does_nothing() {
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);
    assert!(
        surface.clear_registry_state("a", at(0)).is_none(),
        "or a healthy panel would be reset to loading once a second"
    );
    assert_eq!(state_of(&surface, "a"), PanelState::Loading);
}

#[test]
fn a_browser_that_subscribes_is_told_which_generation_is_in_force() {
    // A browser posts its parameters and then opens the stream. The
    // acknowledgement for that post was broadcast before it was listening, so
    // unless the current state carries the generation, it never learns its
    // change landed and reports the surface as pending for as long as the tab
    // is open.
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);
    surface.set_params(7, None, BTreeMap::new(), at(0));

    let frames = surface.current_frames(at(1));

    let acknowledgement = frames
        .iter()
        .find_map(|frame| match frame {
            Frame::Surface(surface) => Some(surface),
            _ => None,
        })
        .expect("the current state names the generation in force");

    assert_eq!(acknowledgement.generation, 7);
    assert!(acknowledgement.acknowledged);
    assert_eq!(
        panels(&frames).len(),
        1,
        "and the panels are still there beside it"
    );
}

#[test]
fn a_time_range_reaches_a_platform_as_something_it_can_parse() {
    // The defect this pins: an RFC 3339 offset carries a `+`, a `+` in a query
    // string means a space, and a platform handed `2026-09-07T18:00:00 00:00`
    // answers 400. The shell then called that `malformed` and blamed a platform
    // that had done nothing wrong. Every panel declaring a time range broke the
    // moment anybody touched the picker.
    let mut surface = surface(vec![instance("a", "orebank", "api/throughput")]);
    surface.set_params(
        2,
        Some(TimeRange {
            from: at(0),
            to: at(3600),
        }),
        BTreeMap::new(),
        at(0),
    );

    let (requests, _) = surface.due(at(1));
    let query = &requests[0].query;

    assert!(
        !query.contains('+'),
        "a raw plus is read as a space by every query parser there is: {query}"
    );
    assert!(
        query.contains("from=") && query.contains("to=") && query.contains("step="),
        "and the window is still there: {query}"
    );
    assert!(
        query.contains("%3A"),
        "the colons of a timestamp are escaped, so the whole value survives: {query}"
    );
}

#[test]
fn a_selection_a_viewer_typed_cannot_break_the_query() {
    let mut surface = surface(vec![accepting(
        "a",
        "orebank",
        "api/throughput",
        &["cluster"],
    )]);

    let mut chosen = BTreeMap::new();
    // Everything a person might reasonably type, and everything that would
    // otherwise be read as query syntax.
    chosen.insert("cluster".to_string(), vec!["north & south=all".to_string()]);
    let mut selections = BTreeMap::new();
    selections.insert("a".to_string(), chosen);
    surface.set_params(2, None, selections, at(0));

    let (requests, _) = surface.due(at(1));
    let query = &requests[0].query;

    assert_eq!(
        query, "cluster=north%20%26%20south%3Dall",
        "one parameter, not three, and the value arrives as it was typed"
    );
}

#[test]
fn two_instances_that_chose_the_same_thing_still_share_a_request() {
    // Encoding must not defeat the deduplication: the sort happens before the
    // escaping, so the same choices written in a different order still produce
    // one query and one fetch.
    let mut surface = surface(vec![
        instance("a", "orebank", "api/throughput"),
        instance("b", "orebank", "api/throughput"),
    ]);

    let mut selections = BTreeMap::new();
    for id in ["a", "b"] {
        let mut chosen = BTreeMap::new();
        chosen.insert("zone".to_string(), vec!["west".to_string()]);
        chosen.insert("cluster".to_string(), vec!["north".to_string()]);
        selections.insert(id.to_string(), chosen);
    }
    surface.set_params(
        2,
        Some(TimeRange {
            from: at(0),
            to: at(60),
        }),
        selections,
        at(0),
    );

    let (requests, _) = surface.due(at(1));
    assert_eq!(requests.len(), 1, "one request, not two");
    assert_eq!(requests[0].instances.len(), 2, "serving both panels");
}

// -- What a viewer may put in a query ------------------------------------

/// An instance that declares the parameters it responds to, as the manifest
/// would.
fn accepting(id: &str, platform: &str, endpoint: &str, params: &[&str]) -> Instance {
    let mut instance = instance(id, platform, endpoint);
    instance.accepts = params.iter().map(|name| name.to_string()).collect();
    instance
}

fn chose(
    id: &str,
    parameter: &str,
    value: &str,
) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    BTreeMap::from([(
        id.to_string(),
        BTreeMap::from([(parameter.to_string(), vec![value.to_string()])]),
    )])
}

#[test]
fn a_selection_the_panel_does_not_declare_never_reaches_the_platform() {
    // Selections arrive from a browser and are stored verbatim with the layout,
    // so before this the shell would put any key a viewer invented onto its own
    // authenticated request: `?admin=true` to a platform that reads its query
    // string, on the shell's credential rather than the viewer's.
    let mut surface = surface(vec![accepting("a", "orebank", "api/x", &["cluster"])]);

    surface.set_params(2, None, chose("a", "injected_by_viewer", "true"), at(0));
    let (requests, _) = surface.due(at(5));

    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0].query.contains("injected_by_viewer"),
        "an undeclared parameter must not be forwarded, got `{}`",
        requests[0].query
    );
}

#[test]
fn a_selection_the_panel_declares_still_reaches_the_platform() {
    // The other half: filtering must not break the feature it guards.
    let mut surface = surface(vec![accepting("a", "orebank", "api/x", &["cluster"])]);

    surface.set_params(2, None, chose("a", "cluster", "eu-west"), at(0));
    let (requests, _) = surface.due(at(5));

    assert_eq!(requests[0].query, "cluster=eu-west");
}

#[test]
fn a_viewer_cannot_overwrite_the_window_the_picker_set() {
    // `from`, `to` and `step` are the shell's to set from the surface's time
    // range. A selection carrying one would let a viewer move the window for
    // that panel behind the picker's back — and, if a platform took the last
    // value, behind the shell's.
    let mut surface = surface(vec![accepting(
        "a",
        "orebank",
        "api/x",
        &["from", "cluster"],
    )]);

    let range = TimeRange {
        from: at(0),
        to: at(3600),
    };
    surface.set_params(
        2,
        Some(range),
        chose("a", "from", "1970-01-01T00:00:00Z"),
        at(0),
    );
    let (requests, _) = surface.due(at(5));

    assert_eq!(
        requests[0].query.matches("from=").count(),
        1,
        "exactly one `from`, and it is the picker's: `{}`",
        requests[0].query
    );
    assert!(
        !requests[0].query.contains("1970"),
        "the viewer's `from` must not appear: `{}`",
        requests[0].query
    );
}

#[test]
fn an_oversized_selection_value_is_dropped_rather_than_sent() {
    // An unbounded value becomes a request line long enough for a platform to
    // refuse, and the shell reports that refusal as `malformed` — blaming a
    // platform that did nothing wrong.
    let mut surface = surface(vec![accepting("a", "orebank", "api/x", &["cluster"])]);

    let huge = "x".repeat(4096);
    surface.set_params(2, None, chose("a", "cluster", &huge), at(0));
    let (requests, _) = surface.due(at(5));

    assert_eq!(
        requests[0].query, "",
        "an oversized value is dropped, leaving no query"
    );
}

#[test]
fn a_panel_that_declares_nothing_accepts_nothing() {
    // The default. Most panels declare no controls at all, and a selections map
    // naming one of them is a browser inventing a parameter.
    let mut surface = surface(vec![instance("a", "orebank", "api/x")]);

    surface.set_params(2, None, chose("a", "cluster", "eu-west"), at(0));
    let (requests, _) = surface.due(at(5));

    assert_eq!(requests[0].query, "");
}

// -- A layout write must not cost a panel its data ------------------------

#[test]
fn a_panel_that_only_moved_keeps_what_it_fetched() {
    // A layout write used to drop the whole surface, so the next subscription
    // rebuilt it and every panel refetched from nothing. Moving one panel a
    // single cell blanked the surface until the answers came back.
    let mut surface = surface(vec![
        instance("a", "orebank", "api/x"),
        instance("b", "orebank", "api/y"),
    ]);

    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(1));
    }
    assert!(
        surface.instance("a").unwrap().envelope().is_some(),
        "both panels start with data"
    );

    // The same two panels, written again — which is what moving one produces.
    let frames = surface.replace_instances(
        vec![
            instance("a", "orebank", "api/x"),
            instance("b", "orebank", "api/y"),
        ],
        at(2),
    );

    assert!(
        surface.instance("a").unwrap().envelope().is_some(),
        "a panel that only moved must keep what it fetched"
    );
    assert_eq!(surface.instance("a").unwrap().state(), PanelState::Ready);

    // And nothing is due, because nothing changed.
    let (due, _) = surface.due(at(3));
    assert!(
        due.is_empty(),
        "a write that changed no data must not refetch: {due:?}"
    );

    // The browser is still told the whole arrangement, since it has just been
    // informed the layout was written and cannot know what survived.
    assert_eq!(panels(&frames).len(), 2);
}

#[test]
fn a_panel_added_by_a_write_is_fetched_and_the_others_are_not() {
    let mut surface = surface(vec![instance("a", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(1));
    }

    surface.replace_instances(
        vec![
            instance("a", "orebank", "api/x"),
            instance("b", "orebank", "api/new"),
        ],
        at(2),
    );

    let (due, _) = surface.due(at(3));
    assert_eq!(due.len(), 1, "only the new panel is due");
    assert_eq!(due[0].endpoint, "api/new");
    assert!(
        surface.instance("a").unwrap().envelope().is_some(),
        "the panel that was already there keeps its data"
    );
}

#[test]
fn a_panel_whose_endpoint_moved_does_not_keep_the_old_answer() {
    // The narrow part of the rule, and the reason it is narrow. A panel that
    // moved is the same panel; one whose platform now serves it from a
    // different endpoint is not, and showing what the old one held under the
    // new one's name would be the shell asserting something nobody told it.
    let mut surface = surface(vec![instance("a", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(1));
    }
    assert!(surface.instance("a").unwrap().envelope().is_some());

    surface.replace_instances(vec![instance("a", "orebank", "api/moved")], at(2));

    assert!(
        surface.instance("a").unwrap().envelope().is_none(),
        "a different endpoint is a different question; the old answer is dropped"
    );

    let (due, _) = surface.due(at(3));
    assert_eq!(due.len(), 1, "and it is asked afresh");
    assert_eq!(due[0].endpoint, "api/moved");
}

#[test]
fn a_panel_removed_by_a_write_is_gone() {
    let mut surface = surface(vec![
        instance("a", "orebank", "api/x"),
        instance("b", "orebank", "api/y"),
    ]);

    let frames = surface.replace_instances(vec![instance("a", "orebank", "api/x")], at(2));

    assert_eq!(surface.instances().len(), 1);
    assert!(surface.instance("b").is_none(), "the removed panel is gone");
    assert_eq!(
        panels(&frames).len(),
        1,
        "and the browser is told about what remains, not what left"
    );
}

// -- A panel's own cadence (HLIN-A-0009) ---------------------------------

fn at_cadence(id: &str, endpoint: &str, refresh: Option<Duration>) -> Instance {
    let mut instance = instance(id, "orebank", endpoint);
    instance.refresh = refresh;
    instance
}

#[test]
fn a_panel_that_says_it_is_slow_is_asked_less_often() {
    // The publisher knows their data's cadence and the shell cannot guess it. A
    // nightly batch count and a live queue depth used to be polled identically,
    // so the operator's one setting was wrong for whichever was not the median.
    let mut surface = surface(vec![
        at_cadence("slow", "api/nightly", Some(Duration::seconds(300))),
        at_cadence("default", "api/ordinary", None),
    ]);

    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(0));
    }

    // The shell's own interval is 30s, so the default panel is due and the one
    // that asked for five minutes is not.
    let (due, _) = surface.due(at(31));
    assert_eq!(due.len(), 1, "only the panel on the shell's cadence");
    assert_eq!(due[0].endpoint, "api/ordinary");

    // And past its own interval it is asked. Asking to be left alone for longer
    // is always honoured, because it costs nobody anything.
    for request in &due {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(31));
    }
    let (due, _) = surface.due(at(301));
    assert!(
        due.iter().any(|request| request.endpoint == "api/nightly"),
        "the slow panel is asked once its own interval has passed: {due:?}"
    );
}

#[test]
fn a_panel_cannot_make_the_shell_poll_faster_than_it_allows() {
    // The half that makes the hint safe. The shell pays for the requests and
    // answers for the load on every platform it fronts, so it keeps the last
    // word (HLIN-A-0009).
    let policy = Policy {
        refresh: Duration::seconds(30),
        refresh_floor: Duration::seconds(5),
        ..Policy::default()
    };
    let mut surface = Surface::new(
        vec![at_cadence(
            "greedy",
            "api/x",
            Some(Duration::milliseconds(1)),
        )],
        policy,
    );

    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(0));
    }

    // One second later it asked to be due, and is not: the floor is five.
    let (due, _) = surface.due(at(1));
    assert!(
        due.is_empty(),
        "a panel asking for a millisecond gets the shell's floor, not a thousand \
         requests a second: {due:?}"
    );

    let (due, _) = surface.due(at(6));
    assert_eq!(due.len(), 1, "and at the floor it is asked");
}

// -- Events (HLIN-S-0006) -------------------------------------------------
//
// The aggregator is where an event becomes a fetch, and it is tested here for
// the reason every other decision in it is: against a clock the test controls,
// with no sockets and no waiting.

use hlin::stream::events::Changed;

fn changed(panel: &str) -> Changed {
    Changed {
        panel: panel.to_string(),
        selections: BTreeMap::new(),
    }
}

fn narrowed(panel: &str, param: &str, value: &str) -> Changed {
    let mut selections = BTreeMap::new();
    selections.insert(param.to_string(), vec![value.to_string()]);
    Changed {
        panel: panel.to_string(),
        selections,
    }
}

/// A surface with one panel that has already been fetched, so the next fetch is
/// governed by an interval rather than by never having asked.
fn settled_surface() -> Surface {
    let mut surface = surface(vec![instance("one", "orebank", "api/x")]);
    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));
    surface
}

#[test]
fn an_event_fetches_a_panel_before_its_cadence_would_have() {
    let mut surface = settled_surface();

    // Nothing is due: the default interval is thirty seconds.
    assert!(surface.due(at(1)).0.is_empty());

    assert_eq!(surface.changed("orebank", &changed("a-panel"), at(1)), 1);

    // Still not immediately — the fetch is spread across the coalescing window,
    // which is what stops one event becoming a burst.
    let (soon, _) = surface.due(at(2));
    assert_eq!(soon.len(), 1, "and well before the thirty seconds it owed");
    assert_eq!(soon[0].endpoint, "api/x");
}

#[test]
fn an_event_about_a_panel_nobody_is_watching_costs_nothing() {
    // REQ-2.2. Otherwise a platform's event rate, rather than the shell's
    // viewers, would set the shell's load.
    let mut surface = settled_surface();

    assert_eq!(
        surface.changed("orebank", &changed("another-panel"), at(1)),
        0
    );
    assert_eq!(surface.changed("elsewhere", &changed("a-panel"), at(1)), 0);
    assert!(
        surface.due(at(2)).0.is_empty(),
        "no fetch follows an event about something not on this surface"
    );
}

#[test]
fn repeated_events_inside_the_window_are_one_fetch() {
    let mut surface = settled_surface();

    assert_eq!(surface.changed("orebank", &changed("a-panel"), at(1)), 1);
    for _ in 0..50 {
        assert_eq!(
            surface.changed("orebank", &changed("a-panel"), at(1)),
            0,
            "an event arriving before the first has been acted on has nothing to add"
        );
    }

    let (requests, _) = surface.due(at(2));
    assert_eq!(requests.len(), 1, "fifty notices, one fetch");
}

#[test]
fn a_flood_of_events_cannot_beat_the_shells_own_floor() {
    // REQ-2.4. A platform emitting a thousand events a second gets exactly what
    // a panel asking to be polled a thousand times a second gets: the shell
    // pays for the requests, so the shell keeps the last word.
    let mut surface = Surface::new(
        vec![instance("one", "orebank", "api/x")],
        Policy {
            refresh_floor: Duration::seconds(5),
            ..Policy::default()
        },
    );

    let (requests, _) = surface.due(at(0));
    surface.resolve(&requests[0], Outcome::Ready(Box::new(envelope())), at(0));

    // Told constantly, for four seconds.
    let mut fetches = 0;
    for second in 1..5 {
        surface.changed("orebank", &changed("a-panel"), at(second));
        fetches += surface.due(at(second)).0.len();
    }
    assert_eq!(fetches, 0, "inside the floor, no amount of telling helps");

    surface.changed("orebank", &changed("a-panel"), at(6));
    assert_eq!(
        surface.due(at(6)).0.len(),
        1,
        "and past it, the news is acted on"
    );
}

#[test]
fn one_event_does_not_fire_every_instance_at_once() {
    // The thundering herd. Twenty panels the tick would have spread across an
    // interval must not become twenty simultaneous requests because one event
    // arrived.
    let instances: Vec<Instance> = (0..20)
        .map(|n| {
            Instance::new(
                format!("i{n}"),
                "orebank",
                "a-panel",
                format!("api/{n}"),
                "scalar.v1",
            )
        })
        .collect();
    let mut surface = surface(instances);

    let (requests, _) = surface.due(at(0));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(0));
    }

    let now = at(1);
    assert_eq!(surface.changed("orebank", &changed("a-panel"), now), 20);

    // At the instant the event arrives, only the instances whose offset has
    // elapsed are due — which is none of them, because the window has not
    // started passing yet.
    let immediately = surface.due(now).0.len();
    assert!(
        immediately < 20,
        "all twenty fired at once: {immediately} of 20"
    );

    // By the end of the window every one of them has been asked, so spreading
    // delays the herd rather than losing part of it.
    let mut asked = immediately;
    asked += surface.due(now + Duration::milliseconds(250)).0.len();
    assert_eq!(asked, 20, "every instance is fetched, just not together");
}

#[test]
fn an_event_narrowed_by_selections_reaches_only_what_it_names() {
    let mut surface = surface(vec![
        instance("west", "orebank", "api/x"),
        instance("east", "orebank", "api/x"),
    ]);

    let mut selections = BTreeMap::new();
    selections.insert(
        "west".to_string(),
        BTreeMap::from([("cluster".to_string(), vec!["west".to_string()])]),
    );
    selections.insert(
        "east".to_string(),
        BTreeMap::from([("cluster".to_string(), vec!["east".to_string()])]),
    );
    surface.set_params(1, None, selections, at(0));

    let (requests, _) = surface.due(at(1));
    for request in &requests {
        surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(1));
    }

    assert_eq!(
        surface.changed("orebank", &narrowed("a-panel", "cluster", "west"), at(2)),
        1,
        "only the instance watching west"
    );

    // And an event naming nothing reaches both, which is the empty case falling
    // out of the subset rule rather than being special-cased.
    assert_eq!(surface.changed("orebank", &changed("a-panel"), at(3)), 1);
}

#[test]
fn a_retired_panel_is_never_fetched_however_loudly_its_platform_shouts() {
    // A layout outlives the panels on it. A platform still announcing one it no
    // longer declares must not make the shell ask for an endpoint that is not
    // in the current manifest.
    let mut surface = surface(vec![Instance::retired("gone", "orebank", "a-panel")]);

    assert_eq!(surface.changed("orebank", &changed("a-panel"), at(1)), 0);
    assert!(surface.due(at(2)).0.is_empty());
}

/// A panel that declares a fast cadence and offers to be reported on — the
/// combination the whole feature exists for.
fn pushed_instance(id: &str, refresh_ms: i64) -> Instance {
    let mut instance = instance(id, "orebank", "api/x");
    instance.refresh = Some(Duration::milliseconds(refresh_ms));
    instance.pushed = true;
    instance
}

/// How many times this instance is fetched over a window of seconds.
fn fetches_over(surface: &mut Surface, seconds: i64) -> usize {
    let mut count = 0;
    for second in 1..=seconds {
        let (requests, _) = surface.due(at(second));
        for request in &requests {
            count += 1;
            surface.resolve(request, Outcome::Ready(Box::new(envelope())), at(second));
        }
    }
    count
}

#[test]
fn a_pushed_panel_on_a_live_stream_polls_at_the_relaxed_interval() {
    // The saving, stated as a number. A panel asking for a quarter-second
    // cadence is polled four times a second when nothing is reporting on it,
    // and at the shell's own interval when something is.
    let mut busy = surface(vec![pushed_instance("one", 250)]);
    let hammering = fetches_over(&mut busy, 60);

    let mut quiet = surface(vec![pushed_instance("one", 250)]);
    quiet.streaming_from("orebank", true);
    let relaxed = fetches_over(&mut quiet, 60);

    assert!(
        relaxed * 10 < hammering,
        "the relaxed interval must be a different order of magnitude: \
         {relaxed} against {hammering} over a minute"
    );
    assert!(
        relaxed >= 2,
        "and still a real interval, not silence: {relaxed}"
    );
}

#[test]
fn losing_the_stream_puts_a_panel_straight_back_on_its_own_cadence() {
    // REQ-1.4, and the reason there is no recovery step: the poll was never
    // turned off, so there is nothing to restart.
    let mut surface = surface(vec![pushed_instance("one", 250)]);
    surface.streaming_from("orebank", true);
    let relaxed = fetches_over(&mut surface, 30);

    surface.streaming_from("orebank", false);
    let after = fetches_over(&mut surface, 30);

    assert!(
        after > relaxed * 5,
        "a panel whose platform stopped reporting must poll like one that never \
         offered to: {after} against {relaxed}"
    );
}

#[test]
fn a_panel_that_is_not_pushed_is_never_relaxed() {
    // A platform may serve a stream and report on only some of its panels. The
    // rest must be polled exactly as before, or the shell would slow down a
    // panel nothing will ever tell it about.
    let quarter_second = |id: &str| {
        let mut one = instance(id, "orebank", "api/x");
        one.refresh = Some(Duration::milliseconds(250));
        one
    };

    // Deliberately not pushed, on a platform that is streaming.
    let mut alongside = surface(vec![quarter_second("one")]);
    alongside.streaming_from("orebank", true);

    let mut alone = surface(vec![quarter_second("one")]);

    assert_eq!(
        fetches_over(&mut alongside, 20),
        fetches_over(&mut alone, 20),
        "a panel nobody promised to report on is polled the same either way"
    );
}

#[test]
fn a_pushed_panel_is_never_polled_more_often_than_it_asked() {
    // The other half of `max`. A panel declaring a slow cadence meant it, and
    // being reported on is not a reason to ask more often than it wanted.
    let mut slow = instance("slow", "orebank", "api/x");
    slow.refresh = Some(Duration::seconds(300));
    slow.pushed = true;

    let mut patient = surface(vec![slow]);
    patient.streaming_from("orebank", true);

    assert_eq!(
        fetches_over(&mut patient, 120),
        1,
        "five minutes means five minutes, whoever is reporting"
    );
}

#[test]
fn a_shell_whose_platforms_offer_nothing_makes_exactly_the_requests_it_used_to() {
    // REQ-1.1, asserted by counting. The overwhelming majority of deployments
    // will never have a platform that declares a stream, and none of them
    // should make a single request more or fewer for this feature existing.
    let mut untouched = surface(vec![
        instance("a", "orebank", "api/a"),
        instance("b", "stampmill", "api/b"),
    ]);

    // A stream connecting for a platform whose panels do not claim to be
    // reported on changes nothing either.
    let mut informed = surface(vec![
        instance("a", "orebank", "api/a"),
        instance("b", "stampmill", "api/b"),
    ]);
    informed.streaming_from("orebank", true);
    informed.streaming_from("stampmill", true);

    assert_eq!(
        fetches_over(&mut untouched, 120),
        fetches_over(&mut informed, 120)
    );
}
