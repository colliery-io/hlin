//! The column, called the way the shell calls it.

use hlin_widget_kanban::{Column, LONGEST, MOST, PANEL, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn kanban() -> Running {
    testing::start(widget(), Column::new("To do"), api()).await
}

fn texts(board: &Value) -> Vec<String> {
    board["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["text"].as_str().unwrap().to_string())
        .collect()
}

fn id(board: &Value, text: &str) -> u64 {
    board["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["text"] == text)
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_table_and_the_column_is_shared() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(panel.pushed, "one column, everybody works it");
}

#[tokio::test]
async fn anyone_adds_at_the_bottom_and_everyone_sees_the_same_column() {
    let kanban = kanban().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let added = kanban
        .write(
            &alice,
            "POST",
            "/api/cards",
            Some(json!({ "text": "  First  " })),
        )
        .await;
    assert_eq!(added.status, 201, "{}", added.body);
    kanban
        .write(
            &bob,
            "POST",
            "/api/cards",
            Some(json!({ "text": "Second" })),
        )
        .await;

    let seen = kanban.get(&bob, "/api/cards").await.body;
    assert_eq!(texts(&seen), ["First", "Second"], "trimmed, in order");
    assert_eq!(seen["cards"][0]["by"], "Alice");
    assert_eq!(seen["cards"][0]["mine"], false);
    assert_eq!(seen["cards"][1]["mine"], true);
}

#[tokio::test]
async fn a_card_says_something_briefly() {
    let kanban = kanban().await;
    let alice = person("u-alice", "Alice");
    for (text, message) in [
        (
            "   ".to_string(),
            "A card needs something on it".to_string(),
        ),
        (
            "x".repeat(LONGEST + 1),
            format!("A card is at most {LONGEST} characters"),
        ),
    ] {
        let refused = kanban
            .write(&alice, "POST", "/api/cards", Some(json!({ "text": text })))
            .await;
        assert_eq!(refused.status, 400);
        assert_eq!(refused.body["message"], message);
    }
}

#[tokio::test]
async fn a_column_holds_twelve() {
    let kanban = kanban().await;
    let alice = person("u-alice", "Alice");
    for n in 0..MOST {
        let added = kanban
            .write(
                &alice,
                "POST",
                "/api/cards",
                Some(json!({ "text": format!("Card {n}") })),
            )
            .await;
        assert_eq!(added.status, 201);
    }
    let refused = kanban
        .write(
            &alice,
            "POST",
            "/api/cards",
            Some(json!({ "text": "One more" })),
        )
        .await;
    assert_eq!(refused.status, 409);
    assert_eq!(
        refused.body["message"],
        "The column holds 12 cards; finish one first"
    );
}

#[tokio::test]
async fn anyone_moves_a_card_one_place_and_not_off_either_end() {
    let kanban = kanban().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    for text in ["A", "B", "C"] {
        kanban
            .write(&alice, "POST", "/api/cards", Some(json!({ "text": text })))
            .await;
    }
    let board = kanban.get(&bob, "/api/cards").await.body;
    let (a, c) = (id(&board, "A"), id(&board, "C"));

    let moved = kanban
        .write(
            &bob,
            "POST",
            &format!("/api/cards/{c}/move"),
            Some(json!({ "by": -1 })),
        )
        .await;
    assert_eq!(moved.status, 200, "Bob may move Alice's card");
    assert_eq!(texts(&moved.body), ["A", "C", "B"]);

    for (card, by, message) in [
        (a, -1, "That card is already at the top"),
        (
            id(&moved.body, "B"),
            1,
            "That card is already at the bottom",
        ),
    ] {
        let refused = kanban
            .write(
                &bob,
                "POST",
                &format!("/api/cards/{card}/move"),
                Some(json!({ "by": by })),
            )
            .await;
        assert_eq!(refused.status, 409);
        assert_eq!(refused.body["message"], message);
    }
    let refused = kanban
        .write(
            &bob,
            "POST",
            &format!("/api/cards/{a}/move"),
            Some(json!({ "by": 2 })),
        )
        .await;
    assert_eq!(refused.status, 400);
}

#[tokio::test]
async fn only_whoever_added_a_card_takes_it_off() {
    let kanban = kanban().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let added = kanban
        .write(
            &alice,
            "POST",
            "/api/cards",
            Some(json!({ "text": "Mine" })),
        )
        .await;
    let card = id(&added.body, "Mine");

    let refused = kanban
        .write(&bob, "DELETE", &format!("/api/cards/{card}"), None)
        .await;
    assert_eq!(refused.status, 403);
    assert_eq!(refused.body["message"], "Only Alice can take this card off");

    let done = kanban
        .write(&alice, "DELETE", &format!("/api/cards/{card}"), None)
        .await;
    assert_eq!(done.status, 200);
    assert!(texts(&done.body).is_empty());
    let gone = kanban
        .write(&alice, "DELETE", &format!("/api/cards/{card}"), None)
        .await;
    assert_eq!(gone.status, 404);
}

#[tokio::test]
async fn a_change_is_announced_and_the_fallback_is_the_column() {
    let kanban = testing::start(widget(), Column::seed(), api()).await;
    let alice = person("u-alice", "Alice");
    let mut events = kanban.listen(&alice).await;
    kanban
        .write(
            &alice,
            "POST",
            "/api/cards",
            Some(json!({ "text": "Ship" })),
        )
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Records(table) = kanban.fallback(&alice).await else {
        panic!("the column's fallback is a table");
    };
    assert_eq!(table.columns[1].label, "This week");
    assert_eq!(table.rows.len(), 4);
    assert_eq!(table.rows[3]["card"], "Ship");
    assert_eq!(table.rows[3]["position"], 4);
}
