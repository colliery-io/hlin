//! The reactions, called the way the shell calls them.

use hlin_widget_reactions::{EMOJI, MOST_EACH, NAMES_SHOWN, PANEL, Reactions, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn reactions() -> Running {
    testing::start(widget(), Reactions::default(), api()).await
}

fn find<'a>(board: &'a Value, id: &str) -> &'a Value {
    board["reactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reaction| reaction["id"] == id)
        .unwrap()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_counts_as_a_pushed_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(panel.pushed);
}

#[tokio::test]
async fn everybody_sees_the_same_counts_and_only_their_own_as_theirs() {
    let reactions = reactions().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let reacted = reactions
        .write(&alice, "PUT", "/api/reactions/party", None)
        .await;
    assert_eq!(reacted.status, 200, "{}", reacted.body);
    reactions
        .write(&bob, "PUT", "/api/reactions/party", None)
        .await;

    let bobs = reactions.get(&bob, "/api/reactions").await.body;
    assert_eq!(find(&bobs, "party")["count"], 2);
    assert_eq!(find(&bobs, "party")["names"], json!(["Alice", "Bob"]));
    assert_eq!(find(&bobs, "party")["mine"], true);
    assert_eq!(find(&bobs, "rocket")["count"], 0);
    assert_eq!(bobs["reactions"].as_array().unwrap().len(), EMOJI.len());
}

#[tokio::test]
async fn one_reaction_per_emoji_per_person_counted_by_id() {
    let reactions = reactions().await;
    let (sam, other_sam) = (person("u-sam-1", "Sam"), person("u-sam-2", "Sam"));

    reactions
        .write(&sam, "PUT", "/api/reactions/heart", None)
        .await;
    let again = reactions
        .write(&sam, "PUT", "/api/reactions/heart", None)
        .await;
    assert_eq!(again.status, 409);
    assert_eq!(
        again.body,
        json!({ "message": "You have already reacted with that" })
    );

    reactions
        .write(&other_sam, "PUT", "/api/reactions/heart", None)
        .await;
    let board = reactions.get(&sam, "/api/reactions").await.body;
    assert_eq!(find(&board, "heart")["count"], 2);
}

#[tokio::test]
async fn nobody_has_more_than_three_at_once() {
    let reactions = reactions().await;
    let alice = person("u-alice", "Alice");

    for (id, _, _) in &EMOJI[..MOST_EACH] {
        let reacted = reactions
            .write(&alice, "PUT", &format!("/api/reactions/{id}"), None)
            .await;
        assert_eq!(reacted.status, 200);
    }
    assert_eq!(
        reactions.get(&alice, "/api/reactions").await.body["left"],
        0
    );

    let one_more = reactions
        .write(&alice, "PUT", "/api/reactions/rocket", None)
        .await;
    assert_eq!(one_more.status, 409);
    assert_eq!(
        one_more.body,
        json!({ "message": "3 reactions each is the most; take one back first" })
    );

    reactions
        .write(&alice, "DELETE", "/api/reactions/thumbs-up", None)
        .await;
    let now_fits = reactions
        .write(&alice, "PUT", "/api/reactions/rocket", None)
        .await;
    assert_eq!(now_fits.status, 200);
}

#[tokio::test]
async fn only_offered_emoji_and_only_your_own_reaction_can_be_taken_back() {
    let reactions = reactions().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let unknown = reactions
        .write(&alice, "PUT", "/api/reactions/pineapple", None)
        .await;
    assert_eq!(unknown.status, 404);

    reactions
        .write(&alice, "PUT", "/api/reactions/eyes", None)
        .await;
    let not_bobs = reactions
        .write(&bob, "DELETE", "/api/reactions/eyes", None)
        .await;
    assert_eq!(not_bobs.status, 404);
    assert_eq!(
        find(&reactions.get(&bob, "/api/reactions").await.body, "eyes")["count"],
        1
    );
}

#[tokio::test]
async fn a_long_list_of_names_is_cut_short() {
    let reactions = reactions().await;
    for n in 0..NAMES_SHOWN + 2 {
        let who = person(&format!("u-{n}"), &format!("Person {n}"));
        reactions
            .write(&who, "PUT", "/api/reactions/laugh", None)
            .await;
    }
    let board = reactions
        .get(&person("u-0", "Person 0"), "/api/reactions")
        .await
        .body;
    assert_eq!(find(&board, "laugh")["count"], NAMES_SHOWN + 2);
    assert_eq!(
        find(&board, "laugh")["names"].as_array().unwrap().len(),
        NAMES_SHOWN
    );
}

#[tokio::test]
async fn a_reaction_is_announced_on_the_event_stream() {
    let reactions = reactions().await;
    let alice = person("u-alice", "Alice");
    let mut events = reactions.listen(&alice).await;

    reactions
        .write(&alice, "PUT", "/api/reactions/rocket", None)
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_every_emoji_and_its_count() {
    let reactions = reactions().await;
    let alice = person("u-alice", "Alice");
    reactions
        .write(&alice, "PUT", "/api/reactions/thumbs-up", None)
        .await;

    let Envelope::Records(table) = reactions.fallback(&alice).await else {
        panic!("the reactions' fallback is a table");
    };
    assert_eq!(table.rows.len(), EMOJI.len());
    assert_eq!(table.rows[0]["reaction"], "👍 Thumbs up");
    assert_eq!(table.rows[0]["count"], 1);
}
