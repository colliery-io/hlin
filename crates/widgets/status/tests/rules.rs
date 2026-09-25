//! The light: its history at chosen instants, and the widget called the way
//! the shell calls it.

use hlin_widget_status::{Light, Lights, PANEL, SERVICES, STRIP, WINDOW, api, light, widget};
use hlin_widget_support::envelope::{Envelope, Health};
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

/// 2026-01-15T12:00:00Z.
const NOON: i64 = 1_768_478_400_000;
const DAY: i64 = 24 * 60 * 60_000 / WINDOW;

async fn status() -> Running {
    testing::start(widget(), Lights::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_status_asked_for_again_as_the_light_moves_on() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("status"));
    assert_eq!(panel.envelope.as_deref(), Some("status.v1"));
    assert_eq!(panel.refresh_ms, Some(30_000));
    assert!(!panel.pushed);
}

#[test]
fn every_service_is_mostly_healthy_and_sometimes_not() {
    let start = NOON / WINDOW;
    for (service, (id, ..)) in SERVICES.iter().enumerate() {
        let week: Vec<Light> = (0..7 * DAY).map(|w| light(service, start + w)).collect();
        let ok = week.iter().filter(|l| **l == Light::Ok).count() as f64 / week.len() as f64;
        assert!(ok > 0.8, "{id} is healthy {ok}");
        assert!(week.contains(&Light::Degraded), "{id}");
        assert!(week.contains(&Light::Down), "{id}");
    }
}

#[test]
fn the_light_is_the_windows_and_the_same_all_through_it() {
    let lights = Lights::default();
    let at = NOON + 1;
    let reading = lights.reading("alice", at);
    assert_eq!(reading.light, light(0, NOON / WINDOW));
    assert_eq!(reading, lights.reading("alice", NOON + WINDOW - 1));
    assert_eq!(reading, lights.reading("bob", at), "the same for everybody");
}

#[test]
fn since_is_where_the_run_began_and_the_strip_ends_now() {
    let lights = Lights::default();
    let reading = lights.reading("alice", NOON);
    let current = NOON / WINDOW;
    let began = reading.since_ms / WINDOW;
    assert!(began <= current && current - began < DAY);
    assert!((began..=current).all(|w| light(0, w) == reading.light));
    if current - began < DAY - 1 {
        assert_ne!(light(0, began - 1), reading.light, "the run began there");
    }
    assert_eq!(reading.strip.len(), STRIP);
    assert_eq!(*reading.strip.last().unwrap(), reading.light);
    assert_eq!(reading.strip[0], light(0, current - STRIP as i64 + 1));
}

#[test]
fn uptime_is_the_share_of_the_day_that_was_healthy() {
    let reading = Lights::default().reading("alice", NOON);
    let current = NOON / WINDOW;
    let healthy = (0..DAY)
        .filter(|back| light(0, current - back) == Light::Ok)
        .count() as f64;
    assert_eq!(
        reading.uptime,
        (healthy * 1000.0 / DAY as f64).round() / 10.0
    );
}

#[test]
fn which_service_is_each_persons_own() {
    let mut lights = Lights::default();
    lights.choose("alice", "search").unwrap();
    assert_eq!(lights.reading("alice", NOON).service, "search");
    assert_eq!(lights.reading("bob", NOON).service, "api");
    let refused = lights.choose("alice", "mainframe").unwrap_err();
    assert_eq!(refused.status, 404);
    assert_eq!(refused.message, "There is no such service");
}

#[tokio::test]
async fn choosing_is_announced_and_the_fallback_is_that_services_light() {
    let status = status().await;
    let alice = person("u-alice", "Alice");
    let mut events = status.listen(&alice).await;
    let chosen = status
        .write(
            &alice,
            "PUT",
            "/api/status/service",
            Some(json!({ "service": "queue" })),
        )
        .await;
    assert_eq!(chosen.status, 200, "{}", chosen.body);
    assert_eq!(chosen.body["service"], "queue");
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Status(light) = status.fallback(&alice).await else {
        panic!("the light's fallback is a status");
    };
    assert_eq!(light.label.as_deref(), Some("Job queue"));
    assert!(matches!(
        light.status,
        Health::Ok | Health::Degraded | Health::Down
    ));
    assert!(light.detail.unwrap().starts_with("Up "));
    assert!(light.since.is_some());
}
