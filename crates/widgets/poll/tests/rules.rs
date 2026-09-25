//! The poll, called the way the shell calls it.

use hlin_widget_poll::{PANEL, Poll, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

async fn poll() -> Running {
    testing::start(widget(), Poll::seed(), api()).await
}

fn votes(tally: &serde_json::Value) -> Vec<u64> {
    tally["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|option| option["votes"].as_u64().unwrap())
        .collect()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_tally_as_a_pushed_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(panel.pushed);
}

#[tokio::test]
async fn everybody_sees_the_same_tally_and_only_their_own_vote() {
    let poll = poll().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let voted = poll
        .write(
            &alice,
            "POST",
            "/api/poll/vote",
            Some(json!({ "option": "kyoto" })),
        )
        .await;
    assert_eq!(voted.status, 200, "{}", voted.body);
    assert_eq!(voted.body["mine"], "kyoto");

    let bobs = poll.get(&bob, "/api/poll").await.body;
    assert_eq!(votes(&bobs), vec![0, 1, 0]);
    assert_eq!(bobs["mine"], serde_json::Value::Null);
    assert_eq!(bobs["total"], 1);
}

#[tokio::test]
async fn a_second_vote_replaces_the_first() {
    let poll = poll().await;
    let alice = person("u-alice", "Alice");

    for option in ["kyoto", "lisbon"] {
        poll.write(
            &alice,
            "POST",
            "/api/poll/vote",
            Some(json!({ "option": option })),
        )
        .await;
    }

    let tally = poll.get(&alice, "/api/poll").await.body;
    assert_eq!(votes(&tally), vec![1, 0, 0]);
    assert_eq!(tally["mine"], "lisbon");
}

#[tokio::test]
async fn votes_are_counted_by_id_not_by_name() {
    let poll = poll().await;
    let (sam, other_sam) = (person("u-sam-1", "Sam"), person("u-sam-2", "Sam"));

    for who in [&sam, &other_sam] {
        poll.write(
            who,
            "POST",
            "/api/poll/vote",
            Some(json!({ "option": "kyoto" })),
        )
        .await;
    }

    assert_eq!(
        votes(&poll.get(&sam, "/api/poll").await.body),
        vec![0, 2, 0]
    );
}

#[tokio::test]
async fn only_an_option_the_poll_offers_can_be_voted_for() {
    let poll = poll().await;
    let alice = person("u-alice", "Alice");

    let refused = poll
        .write(
            &alice,
            "POST",
            "/api/poll/vote",
            Some(json!({ "option": "mars" })),
        )
        .await;
    assert_eq!(refused.status, 404);
    assert_eq!(
        refused.body,
        json!({ "message": "This poll has no such option" })
    );
}

#[tokio::test]
async fn a_vote_can_be_taken_back_once() {
    let poll = poll().await;
    let alice = person("u-alice", "Alice");
    poll.write(
        &alice,
        "POST",
        "/api/poll/vote",
        Some(json!({ "option": "kyoto" })),
    )
    .await;

    let taken = poll.write(&alice, "DELETE", "/api/poll/vote", None).await;
    assert_eq!(taken.status, 200);
    assert_eq!(taken.body["total"], 0);

    let again = poll.write(&alice, "DELETE", "/api/poll/vote", None).await;
    assert_eq!(again.status, 404);
    assert_eq!(again.body, json!({ "message": "You have not voted" }));
}

#[tokio::test]
async fn a_vote_is_announced_on_the_event_stream() {
    let poll = poll().await;
    let alice = person("u-alice", "Alice");
    let mut events = poll.listen(&alice).await;

    poll.write(
        &alice,
        "POST",
        "/api/poll/vote",
        Some(json!({ "option": "kyoto" })),
    )
    .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_the_tally_the_shell_can_draw() {
    let poll = poll().await;
    let alice = person("u-alice", "Alice");
    poll.write(
        &alice,
        "POST",
        "/api/poll/vote",
        Some(json!({ "option": "reykjavik" })),
    )
    .await;

    let Envelope::Records(table) = poll.fallback(&alice).await else {
        panic!("the poll's fallback is a table");
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[2]["option"], "Reykjavík");
    assert_eq!(table.rows[2]["votes"], 1);
}
