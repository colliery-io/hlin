//! The shoutbox, called the way the shell calls it.

use hlin_widget_shoutbox::{KEPT, LONGEST, PANEL, Shoutbox, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn shoutbox() -> Running {
    testing::start(widget(), Shoutbox::default(), api()).await
}

fn texts(room: &Value) -> Vec<String> {
    room["shouts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|shout| shout["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_latest_shouts_as_a_pushed_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(panel.pushed, "one person's shout is everybody's news");
}

#[tokio::test]
async fn everybody_hears_a_shout_and_only_its_author_owns_it() {
    let shoutbox = shoutbox().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let said = shoutbox
        .write(
            &alice,
            "POST",
            "/api/shouts",
            Some(json!({ "text": "  Lunch at one?  " })),
        )
        .await;
    assert_eq!(said.status, 201, "{}", said.body);
    shoutbox
        .write(&bob, "POST", "/api/shouts", Some(json!({ "text": "Yes!" })))
        .await;

    let bobs = shoutbox.get(&bob, "/api/shouts").await.body;
    assert_eq!(texts(&bobs), vec!["Lunch at one?", "Yes!"]);
    assert_eq!(bobs["shouts"][0]["name"], "Alice");
    assert_eq!(bobs["shouts"][0]["mine"], false);
    assert_eq!(bobs["shouts"][1]["mine"], true);
}

#[tokio::test]
async fn a_shout_says_something_and_not_too_much() {
    let shoutbox = shoutbox().await;
    let alice = person("u-alice", "Alice");

    let empty = shoutbox
        .write(
            &alice,
            "POST",
            "/api/shouts",
            Some(json!({ "text": "   " })),
        )
        .await;
    assert_eq!(empty.status, 400);
    assert_eq!(empty.body, json!({ "message": "Say something first" }));

    let long = "é".repeat(LONGEST + 1);
    let refused = shoutbox
        .write(&alice, "POST", "/api/shouts", Some(json!({ "text": long })))
        .await;
    assert_eq!(refused.status, 400);

    let just = "é".repeat(LONGEST);
    let fits = shoutbox
        .write(&alice, "POST", "/api/shouts", Some(json!({ "text": just })))
        .await;
    assert_eq!(fits.status, 201, "characters, not bytes");
}

#[tokio::test]
async fn the_same_thing_twice_in_a_row_is_refused_but_not_from_someone_else() {
    let shoutbox = shoutbox().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let hello = Some(json!({ "text": "hello" }));

    shoutbox
        .write(&alice, "POST", "/api/shouts", hello.clone())
        .await;
    let twice = shoutbox
        .write(&alice, "POST", "/api/shouts", hello.clone())
        .await;
    assert_eq!(twice.status, 409);
    assert_eq!(twice.body, json!({ "message": "You just said that" }));

    let echoed = shoutbox.write(&bob, "POST", "/api/shouts", hello).await;
    assert_eq!(echoed.status, 201);
}

#[tokio::test]
async fn the_room_remembers_only_the_latest() {
    let shoutbox = shoutbox().await;
    let alice = person("u-alice", "Alice");
    for n in 0..KEPT + 5 {
        shoutbox
            .write(
                &alice,
                "POST",
                "/api/shouts",
                Some(json!({ "text": format!("shout {n}") })),
            )
            .await;
    }

    let kept = texts(&shoutbox.get(&alice, "/api/shouts").await.body);
    assert_eq!(kept.len(), KEPT);
    assert_eq!(kept[0], "shout 5");
    assert_eq!(kept[KEPT - 1], format!("shout {}", KEPT + 4));
}

#[tokio::test]
async fn only_whoever_said_it_can_take_a_shout_back() {
    let shoutbox = shoutbox().await;
    let (sam, other_sam) = (person("u-sam-1", "Sam"), person("u-sam-2", "Sam"));
    let said = shoutbox
        .write(&sam, "POST", "/api/shouts", Some(json!({ "text": "oops" })))
        .await;
    let id = said.body["shouts"][0]["id"].as_u64().unwrap();
    let path = format!("/api/shouts/{id}");

    let refused = shoutbox.write(&other_sam, "DELETE", &path, None).await;
    assert_eq!(refused.status, 403, "the same name is not the same person");

    let taken = shoutbox.write(&sam, "DELETE", &path, None).await;
    assert_eq!(taken.status, 200);
    assert!(texts(&taken.body).is_empty());

    let gone = shoutbox.write(&sam, "DELETE", &path, None).await;
    assert_eq!(gone.status, 404);
}

#[tokio::test]
async fn a_retried_shout_is_said_once() {
    let shoutbox = shoutbox().await;
    let alice = person("u-alice", "Alice");
    let body = Some(json!({ "text": "once" }));

    shoutbox
        .write_keyed(&alice, "POST", "/api/shouts", body.clone(), "k1")
        .await;
    let again = shoutbox
        .write_keyed(&alice, "POST", "/api/shouts", body, "k1")
        .await;

    assert!(again.replayed);
    assert_eq!(
        texts(&shoutbox.get(&alice, "/api/shouts").await.body),
        vec!["once"]
    );
}

#[tokio::test]
async fn a_shout_is_announced_on_the_event_stream() {
    let shoutbox = shoutbox().await;
    let alice = person("u-alice", "Alice");
    let mut events = shoutbox.listen(&alice).await;

    shoutbox
        .write(&alice, "POST", "/api/shouts", Some(json!({ "text": "hi" })))
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_the_latest_shouts_newest_first() {
    let shoutbox = shoutbox().await;
    let alice = person("u-alice", "Alice");
    for text in ["first", "second"] {
        shoutbox
            .write(&alice, "POST", "/api/shouts", Some(json!({ "text": text })))
            .await;
    }

    let Envelope::Records(table) = shoutbox.fallback(&alice).await else {
        panic!("the shoutbox's fallback is a table");
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0]["text"], "second");
    assert_eq!(table.rows[0]["name"], "Alice");
}
