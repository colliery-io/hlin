//! The clocks, called the way the shell calls them.

use chrono::{TimeZone, Utc};
use hlin_widget_clock::{Clock, Clocks, PANEL, api, reading, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

async fn clocks() -> Running {
    testing::start(widget(), Clocks::default(), api()).await
}

fn ids(board: &serde_json::Value) -> Vec<&str> {
    board["cities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|city| city["id"].as_str().unwrap())
        .collect()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_panel_is_not_pushed_because_nobody_else_is_told() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert!(!panel.pushed);
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.refresh_ms, Some(30_000));
}

#[test]
fn offsets_follow_daylight_saving() {
    let alice = hlin_widget_support::Claims {
        iss: "hlin".to_string(),
        sub: "u-alice".to_string(),
        aud: PANEL.to_string(),
        iat: 0,
        exp: 0,
        jti: "t".to_string(),
        name: None,
        email: None,
        groups: vec![],
        extra: Default::default(),
    };
    let clocks = Clocks::default();
    let winter = clocks.board(&alice, Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap());
    let summer = clocks.board(&alice, Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap());

    let london = |board: &hlin_widget_clock::Board| board.cities[0].offset_minutes;
    assert_eq!(london(&winter), 0);
    assert_eq!(london(&summer), 60);
    assert_eq!(
        winter.cities[2].offset_minutes,
        9 * 60,
        "Tokyo keeps no summer time"
    );
}

#[test]
fn a_reading_is_the_local_time_and_the_offset() {
    let tokyo = Clock {
        id: "tokyo",
        name: "Tokyo",
        zone: "Asia/Tokyo".to_string(),
        offset_minutes: 540,
    };
    let noon = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
    assert_eq!(
        reading(&tokyo, noon),
        ("21:00".to_string(), "+09:00".to_string())
    );

    let st_johns = Clock {
        offset_minutes: -210,
        ..tokyo
    };
    assert_eq!(reading(&st_johns, noon).1, "-03:30");
}

#[tokio::test]
async fn everybody_starts_with_three_cities() {
    let clocks = clocks().await;
    let board = clocks
        .get(&person("u-alice", "Alice"), "/api/clocks")
        .await
        .body;
    assert_eq!(ids(&board), vec!["london", "new-york", "tokyo"]);
    assert!(
        board["others"]
            .as_array()
            .unwrap()
            .iter()
            .all(|other| !["london", "new-york", "tokyo"].contains(&other["id"].as_str().unwrap()))
    );
}

#[tokio::test]
async fn one_persons_clocks_are_theirs_alone() {
    let clocks = clocks().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let added = clocks
        .write(
            &alice,
            "POST",
            "/api/clocks",
            Some(json!({ "city": "lagos" })),
        )
        .await;
    assert_eq!(added.status, 201, "{}", added.body);
    assert_eq!(
        ids(&added.body),
        vec!["london", "new-york", "tokyo", "lagos"]
    );

    let removed = clocks
        .write(&alice, "DELETE", "/api/clocks/london", None)
        .await;
    assert_eq!(ids(&removed.body), vec!["new-york", "tokyo", "lagos"]);

    let bobs = clocks.get(&bob, "/api/clocks").await.body;
    assert_eq!(ids(&bobs), vec!["london", "new-york", "tokyo"]);
}

#[tokio::test]
async fn only_known_cities_and_not_twice() {
    let clocks = clocks().await;
    let alice = person("u-alice", "Alice");

    let unknown = clocks
        .write(
            &alice,
            "POST",
            "/api/clocks",
            Some(json!({ "city": "atlantis" })),
        )
        .await;
    assert_eq!(unknown.status, 404);
    assert_eq!(
        unknown.body,
        json!({ "message": "There is no clock for that city" })
    );

    let twice = clocks
        .write(
            &alice,
            "POST",
            "/api/clocks",
            Some(json!({ "city": "tokyo" })),
        )
        .await;
    assert_eq!(twice.status, 409);
    assert_eq!(
        twice.body,
        json!({ "message": "That clock is already on your list" })
    );
}

#[tokio::test]
async fn at_most_six_and_at_least_one() {
    let clocks = clocks().await;
    let alice = person("u-alice", "Alice");

    for city in ["lagos", "berlin", "mumbai"] {
        let added = clocks
            .write(&alice, "POST", "/api/clocks", Some(json!({ "city": city })))
            .await;
        assert_eq!(added.status, 201, "{city}");
    }
    let seventh = clocks
        .write(
            &alice,
            "POST",
            "/api/clocks",
            Some(json!({ "city": "sydney" })),
        )
        .await;
    assert_eq!(seventh.status, 409);
    assert_eq!(seventh.body, json!({ "message": "Six clocks is the most" }));

    for city in ["london", "new-york", "tokyo", "lagos", "berlin"] {
        let path = format!("/api/clocks/{city}");
        assert_eq!(
            clocks.write(&alice, "DELETE", &path, None).await.status,
            200
        );
    }
    let last = clocks
        .write(&alice, "DELETE", "/api/clocks/mumbai", None)
        .await;
    assert_eq!(last.status, 409);
    assert_eq!(last.body, json!({ "message": "Keep at least one clock" }));

    let absent = clocks
        .write(&alice, "DELETE", "/api/clocks/tokyo", None)
        .await;
    assert_eq!(absent.status, 404);
}

#[tokio::test]
async fn a_change_is_still_announced_for_the_same_persons_other_browsers() {
    let clocks = clocks().await;
    let alice = person("u-alice", "Alice");
    let mut events = clocks.listen(&alice).await;

    clocks
        .write(
            &alice,
            "POST",
            "/api/clocks",
            Some(json!({ "city": "lagos" })),
        )
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_this_persons_clocks_as_a_table() {
    let clocks = clocks().await;
    let alice = person("u-alice", "Alice");

    let Envelope::Records(table) = clocks.fallback(&alice).await else {
        panic!("the clock's fallback is a table");
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0]["city"], "London");
    assert!(table.as_of.is_some());
}
