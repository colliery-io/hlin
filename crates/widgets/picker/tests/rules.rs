//! The picker, called the way the shell calls it, and its draw with dice the
//! test chooses.

use hlin_widget_picker::{PANEL, Picker, REMEMBERED, api, widget};
use hlin_widget_support::Claims;
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn picker() -> Running {
    testing::start(widget(), Picker::default(), api()).await
}

fn claims(sub: &str, name: &str) -> Claims {
    Claims {
        iss: "hlin".to_string(),
        sub: sub.to_string(),
        aud: PANEL.to_string(),
        iat: 0,
        exp: 0,
        jti: "t".to_string(),
        name: Some(name.to_string()),
        email: None,
        groups: Vec::new(),
        extra: Default::default(),
    }
}

fn names(team: &Value) -> Vec<String> {
    team["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_latest_picks_as_a_pushed_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(panel.pushed);
}

#[test]
fn a_pick_lands_only_on_someone_in_the_draw() {
    let mut picker = Picker::default();
    let (alice, bob, carol) = (
        claims("u-alice", "Alice"),
        claims("u-bob", "Bob"),
        claims("u-carol", "Carol"),
    );
    for who in [&alice, &bob, &carol] {
        picker.saw(who);
    }
    picker.set_in_draw(&bob, false);

    for roll in 0..50 {
        let picked = picker.pick(&alice, roll, 0).unwrap().name.clone();
        assert_ne!(picked, "Bob", "Bob sat out");
    }
}

#[test]
fn nobody_is_picked_twice_running_while_there_is_anyone_else() {
    let mut picker = Picker::default();
    let (alice, bob) = (claims("u-alice", "Alice"), claims("u-bob", "Bob"));
    picker.saw(&bob);

    let mut previous = picker.pick(&alice, 0, 0).unwrap().name.clone();
    for roll in 1..20 {
        let picked = picker.pick(&alice, roll, 0).unwrap().name.clone();
        assert_ne!(picked, previous);
        previous = picked;
    }

    // Alone in the draw, the same person every time is the only choice.
    picker.set_in_draw(&bob, false);
    assert_eq!(picker.pick(&alice, 3, 0).unwrap().name, "Alice");
    assert_eq!(picker.pick(&alice, 4, 0).unwrap().name, "Alice");
}

#[test]
fn nobody_in_the_draw_is_no_pick() {
    let mut picker = Picker::default();
    let alice = claims("u-alice", "Alice");
    picker.set_in_draw(&alice, false);
    let refused = picker.pick(&alice, 0, 0).unwrap_err();
    assert_eq!(
        refused.message,
        "Nobody is in the draw; somebody has to be in it to be picked"
    );
}

#[test]
fn the_picker_remembers_only_the_latest_picks_newest_first() {
    let mut picker = Picker::default();
    let alice = claims("u-alice", "Alice");
    for at in 0..REMEMBERED as i64 + 3 {
        picker.pick(&alice, 0, at).unwrap();
    }
    let team = picker.team(&alice);
    assert_eq!(team.picks.len(), REMEMBERED);
    assert_eq!(team.picks[0].at_ms, REMEMBERED as i64 + 2);
}

#[test]
fn a_person_is_known_by_id_and_their_name_kept_current() {
    let picker = Picker::default();
    assert!(picker.saw(&claims("u-sam-1", "Sam")));
    assert!(picker.saw(&claims("u-sam-2", "Sam")));
    assert!(!picker.saw(&claims("u-sam-1", "Samantha")));

    let team = picker.team(&claims("u-sam-1", "Samantha"));
    let names: Vec<_> = team.people.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["Sam", "Samantha"]);
    assert!(team.people[1].me);
}

#[tokio::test]
async fn whoever_opens_the_picker_is_in_the_team() {
    let picker = picker().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    assert_eq!(
        names(&picker.get(&alice, "/api/picker").await.body),
        vec!["Alice"]
    );
    let bobs = picker.get(&bob, "/api/picker").await.body;
    assert_eq!(names(&bobs), vec!["Alice", "Bob"]);
    assert_eq!(bobs["in_draw"], 2);
    assert_eq!(bobs["people"][1]["me"], true);
}

#[tokio::test]
async fn somebody_new_is_announced_to_everybody_else() {
    let picker = picker().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let mut events = picker.listen(&alice).await;

    picker.get(&bob, "/api/picker").await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn a_pick_through_the_shell_lands_on_someone_who_has_been_here() {
    let picker = picker().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    picker.get(&bob, "/api/picker").await;

    let picked = picker.write(&alice, "POST", "/api/picker/pick", None).await;
    assert_eq!(picked.status, 200, "{}", picked.body);
    let name = picked.body["picks"][0]["name"].as_str().unwrap();
    assert!(["Alice", "Bob"].contains(&name), "{name}");
    assert_eq!(picked.body["picks"][0]["by"], "Alice");
    assert!(
        picked.body["picks"][0].get("sub").is_none(),
        "ids are the picker's own business"
    );
}

#[tokio::test]
async fn each_person_sits_out_only_themselves() {
    let picker = picker().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    picker.get(&alice, "/api/picker").await;

    let out = picker
        .write(&bob, "PUT", "/api/picker/me", Some(json!({ "in": false })))
        .await;
    assert_eq!(out.status, 200);
    assert_eq!(out.body["in_draw"], 1);

    for _ in 0..10 {
        let picked = picker.write(&alice, "POST", "/api/picker/pick", None).await;
        assert_eq!(picked.body["picks"][0]["name"], "Alice");
    }

    let nonsense = picker
        .write(
            &bob,
            "PUT",
            "/api/picker/me",
            Some(json!({ "in": "maybe" })),
        )
        .await;
    assert_eq!(nonsense.status, 400);
}

#[tokio::test]
async fn a_pick_is_announced_on_the_event_stream() {
    let picker = picker().await;
    let alice = person("u-alice", "Alice");
    picker.get(&alice, "/api/picker").await;
    let mut events = picker.listen(&alice).await;

    picker.write(&alice, "POST", "/api/picker/pick", None).await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_the_latest_picks() {
    let picker = picker().await;
    let alice = person("u-alice", "Alice");
    picker.write(&alice, "POST", "/api/picker/pick", None).await;

    let Envelope::Records(table) = picker.fallback(&alice).await else {
        panic!("the picker's fallback is a table");
    };
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0]["name"], "Alice");
    assert_eq!(table.rows[0]["by"], "Alice");
}
