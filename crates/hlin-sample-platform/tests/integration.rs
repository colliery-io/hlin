//! The reference platform, checked against the contracts it claims to follow.
//!
//! A sample platform that does not satisfy `hlin-manifest` would teach every
//! team that copies it to be wrong, so these tests are less about this binary
//! working and more about the reference being true.

use hlin_manifest::envelope::Envelope;
use hlin_manifest::{Class, classify_diff, parse_envelope, validate};
use hlin_sample_platform::data::{self, Window};
use hlin_sample_platform::manifest;

use chrono::{Duration, Utc};

const PLATFORM: &str = "orebank";

// -- The manifest is one the shell accepts --------------------------------

#[test]
fn the_reference_manifest_is_valid() {
    let document = manifest::build(PLATFORM, false);
    let checked = validate(&document, PLATFORM);

    assert!(
        checked.is_document_valid(),
        "document defect: {:?}",
        checked.document
    );
    assert!(
        checked.rejected().is_empty(),
        "a platform teams copy must have no rejected panels: {:?}",
        checked.rejected()
    );
    // Every panel declared, accepted. Counted from the document rather than
    // written down, because a number here asserts nothing about validation
    // beyond how many panels this platform happened to have on the day it was
    // written — and fails the moment one is added, which is not a defect.
    assert_eq!(
        checked.accepted_keys().len(),
        document.panels.len(),
        "a platform team's copy must have every panel accepted"
    );
}

#[test]
fn the_manifest_round_trips_through_the_contract_crate() {
    let document = manifest::build(PLATFORM, false);
    let json = serde_json::to_string(&document).expect("serialises");
    let parsed = hlin_manifest::parse_str(&json).expect("parses");
    assert_eq!(document, parsed);
}

#[test]
fn the_manifest_covers_the_whole_envelope_vocabulary() {
    let document = manifest::build(PLATFORM, false);
    let declared: Vec<&str> = document
        .panels
        .iter()
        .map(|panel| panel.envelope.as_str())
        .collect();

    for envelope in ["scalar.v1", "series.v1", "records.v1", "status.v1"] {
        assert!(
            declared.contains(&envelope),
            "the reference should demonstrate {envelope}"
        );
    }
}

#[test]
fn two_panels_share_one_endpoint_so_dedup_is_visible() {
    // The shell deduplicates identical requests. That is invisible unless a
    // surface can hold two panels that ask for the same thing, so the
    // reference provides exactly that.
    let document = manifest::build(PLATFORM, false);
    let throughput = document.panel("throughput").expect("panel exists");
    let compact = document.panel("throughput-compact").expect("panel exists");

    assert_eq!(throughput.data, compact.data);
    assert_eq!(throughput.envelope, compact.envelope);
    assert_ne!(
        throughput.kind, compact.kind,
        "the same data drawn two ways is the point"
    );
}

#[test]
fn a_select_panel_names_an_options_endpoint() {
    let document = manifest::build(PLATFORM, false);
    let panel = document
        .panel("throughput-by-cluster")
        .expect("panel exists");

    let select = panel
        .params
        .iter()
        .find(|declaration| declaration.param == "select")
        .expect("declares a select");

    assert_eq!(
        select.config.get("options").and_then(|v| v.as_str()),
        Some("api/hlin/orebank-clusters")
    );
}

// -- The breaking manifest is breaking ------------------------------------

#[test]
fn the_breaking_manifest_is_a_violation_the_shell_can_catch() {
    let before = manifest::build(PLATFORM, false);
    let after = manifest::build(PLATFORM, true);

    let report = classify_diff(&before, &after);
    assert_eq!(report.class, Class::Breaking);
    assert!(
        matches!(report.verdict, hlin_manifest::Verdict::Violation { .. }),
        "the version is unchanged, so this must read as a violation: {:?}",
        report.verdict
    );

    // And it is still a valid document, so the shell sees a contract change
    // rather than a malformed platform.
    let checked = validate(&after, PLATFORM);
    assert!(checked.is_document_valid());
    assert!(checked.rejected().is_empty());
}

// -- Every endpoint returns what its panel promised -----------------------

fn window() -> Window {
    let to = Utc::now();
    Window::new(Some(to - Duration::hours(1)), Some(to), Some(60))
}

fn accepted(envelope: &Envelope, promised: &str) {
    let json = serde_json::to_vec(envelope).expect("serialises");
    parse_envelope(&json, promised)
        .unwrap_or_else(|defect| panic!("{promised} should be accepted: {defect}"));
}

#[test]
fn every_endpoint_returns_the_envelope_its_panel_declared() {
    accepted(&data::records_per_second(PLATFORM), "scalar.v1");
    accepted(&data::throughput(PLATFORM, window()), "series.v1");
    accepted(&data::throughput_by_cluster(PLATFORM, None), "series.v1");
    accepted(&data::queue_depth(PLATFORM), "records.v1");
    accepted(&data::worker_health(PLATFORM), "status.v1");
    accepted(&data::cluster_options(PLATFORM), "options.v1");
}

#[test]
fn the_step_hint_is_honoured() {
    let to = Utc::now();
    let hour = Window::new(Some(to - Duration::hours(1)), Some(to), Some(60));

    let Envelope::Series(series) = data::throughput(PLATFORM, hour) else {
        panic!("expected a series")
    };

    let points = series.series[0].points.len();
    assert!(
        (55..=65).contains(&points),
        "an hour at one point per minute should be about 60 points, got {points}"
    );
}

#[test]
fn a_coarser_step_returns_fewer_points() {
    let to = Utc::now();
    let fine = Window::new(Some(to - Duration::hours(1)), Some(to), Some(60));
    let coarse = Window::new(Some(to - Duration::hours(1)), Some(to), Some(600));

    let count = |window| {
        let Envelope::Series(series) = data::throughput(PLATFORM, window) else {
            panic!("expected a series")
        };
        series.series[0].points.len()
    };

    assert!(count(fine) > count(coarse));
}

#[test]
fn every_series_carries_a_gap_so_the_rendering_of_one_is_visible() {
    let Envelope::Series(series) = data::throughput(PLATFORM, window()) else {
        panic!("expected a series")
    };

    for line in &series.series {
        assert!(
            line.points.iter().any(|point| point.value().is_none()),
            "series `{}` should demonstrate a gap",
            line.name
        );
    }
}

#[test]
fn data_is_deterministic_so_the_demo_is_reproducible() {
    // The values are stable for a fixed window; `as_of` is deliberately not,
    // because it says when the data was true and the answer is now.
    let to = Utc::now();
    let fixed = Window::new(Some(to - Duration::hours(1)), Some(to), Some(60));

    let points = || {
        let Envelope::Series(series) = data::throughput(PLATFORM, fixed) else {
            panic!("expected a series")
        };
        series.series
    };

    assert_eq!(points(), points());
}

#[test]
fn two_platforms_produce_different_data() {
    let fixed = window();
    let Envelope::Series(one) = data::throughput("orebank", fixed) else {
        panic!("expected a series")
    };
    let Envelope::Series(other) = data::throughput("stampmill", fixed) else {
        panic!("expected a series")
    };

    assert_ne!(one.series[0].name, other.series[0].name);
    assert_ne!(one.series[0].points, other.series[0].points);
}

#[test]
fn the_select_narrows_what_comes_back() {
    let clusters = data::clusters(PLATFORM);

    // Compared by the level each cluster sits at rather than point for point.
    // This endpoint answers a window that ends *now*, so two calls never carry
    // the same timestamps and comparing the arrays would only ever be testing
    // how fast the machine is. What identifies a cluster is the level, and that
    // holds whenever the call was made.
    let level = |cluster: Option<&str>| {
        let Envelope::Series(series) = data::throughput_by_cluster(PLATFORM, cluster) else {
            panic!("expected a series")
        };
        let points = &series.series[0].points;
        let total: f64 = points.iter().filter_map(|point| point.1).sum();
        (total / points.len() as f64).round()
    };

    // Compared with a tolerance, because the window ends at *now*: two calls
    // are two slightly different windows, so the same cluster's level drifts a
    // little between them. Clusters sit ninety apart by construction, so a few
    // units of drift cannot be mistaken for a different cluster — and asserting
    // exact equality made this fail only under load, which is the worst kind of
    // test to own.
    let drift = 20.0;

    assert!(
        (level(None) - level(Some(&clusters[1].0))).abs() > drift,
        "choosing a different cluster should change the data"
    );

    // An unknown selection falls back to the platform's own default rather
    // than failing, which is what the parameter vocabulary asks of a platform.
    assert!(
        (level(Some("no-such-cluster")) - level(None)).abs() < drift,
        "an unknown cluster should give the platform's default back"
    );
}

#[test]
fn the_options_endpoint_offers_what_the_select_accepts() {
    let Envelope::Options(options) = data::cluster_options(PLATFORM) else {
        panic!("expected options")
    };
    let offered: Vec<&str> = options
        .options
        .iter()
        .map(|choice| choice.value.as_str())
        .collect();

    for (value, _) in data::clusters(PLATFORM) {
        assert!(offered.contains(&value.as_str()));
    }
}

// -- Identity, end to end -------------------------------------------------

#[test]
fn a_platform_in_token_mode_accepts_only_tokens_meant_for_it() {
    // The two crates meeting: the shell mints, the platform verifies. This is
    // the check every real platform will perform, so the reference must show
    // it working and show it refusing.
    use hlin_identity::{Issuer, Principal, Verifier};

    let shell = Issuer::generate("hlin");
    let verifier = Verifier::with_keys("hlin", shell.jwks());
    let person = Principal::new("u_dev").with_groups(["oncall"]);

    let for_us = shell.mint(&person, PLATFORM).expect("mints");
    let claims = verifier.verify(&for_us, PLATFORM).expect("accepted");
    assert_eq!(claims.sub, "u_dev");
    assert!(
        claims.in_group("oncall"),
        "groups reach the platform intact"
    );

    let for_someone_else = shell.mint(&person, "stampmill").expect("mints");
    assert!(
        verifier.verify(&for_someone_else, PLATFORM).is_err(),
        "a token for another platform must not open this one"
    );
}

// -- The event stream (HLIN-S-0006) ---------------------------------------

/// This platform, on a real port.
///
/// A real socket rather than `oneshot`, because what is under test is a
/// response that never ends: an in-process call would return the stream's first
/// chunk and prove nothing about holding it open.
async fn serving() -> (
    String,
    std::sync::Arc<hlin_sample_platform::changes::Changes>,
) {
    let changes = hlin_sample_platform::changes::Changes::new();
    let config = hlin_sample_platform::Config {
        name: PLATFORM.to_string(),
        breaking: false,
        identity: hlin_sample_platform::IdentityMode::Open,
        restricted_panel_group: None,
        changes: changes.clone(),
    };

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, hlin_sample_platform::router(config))
            .await
            .unwrap();
    });

    (base, changes)
}

/// Read from a held-open response until `looking_for` appears, or give up.
async fn watch_for(
    response: reqwest::Response,
    looking_for: &str,
    patience: std::time::Duration,
) -> Option<String> {
    use futures::StreamExt;

    let mut body = response.bytes_stream();
    let mut seen = String::new();

    let deadline = tokio::time::Instant::now() + patience;
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout_at(deadline, body.next()).await;
        match next {
            Ok(Some(Ok(chunk))) => {
                seen.push_str(&String::from_utf8_lossy(&chunk));
                if seen.contains(looking_for) {
                    return Some(seen);
                }
            }
            Ok(Some(Err(_))) | Ok(None) => return None,
            Err(_) => return None,
        }
    }
    None
}

#[tokio::test]
async fn the_manifest_says_where_its_events_are_and_which_panels_it_reports_on() {
    let document = manifest::build(PLATFORM, false);

    assert_eq!(document.events.as_deref(), Some("api/events"));

    let pushed: Vec<&str> = document
        .panels
        .iter()
        .filter(|panel| panel.pushed)
        .map(|panel| panel.key.as_str())
        .collect();

    let mut declared = pushed.clone();
    declared.sort_unstable();
    let mut kept = hlin_sample_platform::changes::PUSHED_PANELS.to_vec();
    kept.sort_unstable();

    assert_eq!(
        declared, kept,
        "only the panels whose data this platform actually keeps may claim to be \
         reported on; the rest are derived from the clock and it could not honestly \
         say when they changed"
    );
}

#[tokio::test]
async fn a_change_reaches_a_subscriber_as_an_event() {
    let (base, changes) = serving().await;

    let stream = reqwest::Client::new()
        .get(format!("{base}/api/events"))
        .send()
        .await
        .expect("the stream opens");

    assert_eq!(stream.status(), 200);
    assert_eq!(
        stream
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream"),
        "an SSE endpoint that does not say so is one no client will treat as one"
    );

    // Given after the subscription exists, which is the order a shell relies on.
    changes.completed_a_batch();

    let seen = watch_for(stream, "changed", std::time::Duration::from_secs(5))
        .await
        .expect("the change was announced");

    assert!(seen.contains("event: changed"), "{seen}");
    assert!(seen.contains(r#""panel":"batches""#), "{seen}");
    assert!(
        !seen.contains("value"),
        "an event must carry news, not data — a frame is a property of a panel \
         *and* its viewer, which this platform cannot know: {seen}"
    );
}

#[tokio::test]
async fn the_data_is_already_changed_when_the_event_arrives() {
    // The race a push-capable platform has to get right. A shell that refetches
    // the instant it is told must not read the old value and then wait for the
    // relaxed interval before trying again.
    let (base, changes) = serving().await;

    let stream = reqwest::Client::new()
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();

    changes.completed_a_batch();
    watch_for(stream, "changed", std::time::Duration::from_secs(5))
        .await
        .expect("the change was announced");

    let value: serde_json::Value = reqwest::get(format!("{base}/api/hlin/batches"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        value.get("value").and_then(serde_json::Value::as_u64),
        Some(1),
        "the write must land before the announcement: {value}"
    );
}

#[tokio::test]
async fn many_shells_may_watch_one_platform() {
    // A shell reconnects, and a test suite subscribes repeatedly. Subscribers
    // must not interfere with one another.
    let (base, changes) = serving().await;

    let mut streams = Vec::new();
    for _ in 0..4 {
        streams.push(
            reqwest::Client::new()
                .get(format!("{base}/api/events"))
                .send()
                .await
                .unwrap(),
        );
    }

    changes.completed_a_batch();

    for stream in streams {
        assert!(
            watch_for(stream, "changed", std::time::Duration::from_secs(5))
                .await
                .is_some(),
            "every subscriber hears about it"
        );
    }
}

#[tokio::test]
async fn a_quiet_platform_still_says_it_is_there() {
    // The heartbeat, and the reason it is required rather than encouraged: a
    // connection can stop delivering without closing, and without this there is
    // nothing on the wire to tell a healthy silent stream from a dead one.
    let (base, _changes) = serving().await;

    let stream = reqwest::Client::new()
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();

    // Nothing is changed here on purpose. The only thing that can arrive is the
    // keep-alive.
    let seen = watch_for(stream, ":", std::time::Duration::from_secs(25))
        .await
        .expect("a stream with nothing to say still says something");

    assert!(
        seen.trim_start().starts_with(':'),
        "a heartbeat is an SSE comment, which a client ignores as data and reads \
         as liveness: {seen:?}"
    );
}
