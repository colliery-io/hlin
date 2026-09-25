//! The quote: its rotation at chosen hours, and the widget called the way the
//! shell calls it.

use hlin_widget_quote::{PANEL, QUOTES, Quotes, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

async fn quote() -> Running {
    testing::start(widget(), Quotes::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_table_and_the_quote_is_nobody_elses() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(!panel.pushed);
}

#[test]
fn everybody_sees_the_quote_of_the_hour_and_it_moves_on_by_itself() {
    let quotes = Quotes::default();
    assert_eq!(quotes.shown("alice", 5), quotes.shown("bob", 5));
    assert_eq!(quotes.shown("alice", 5).id, 5 % QUOTES.len());
    assert_ne!(quotes.shown("alice", 5).id, quotes.shown("alice", 6).id);
    assert_eq!(
        quotes.shown("alice", 0).id,
        quotes.shown("alice", QUOTES.len() as u64).id,
        "round and round"
    );
}

#[test]
fn skipping_moves_only_your_own_quote_on() {
    let mut quotes = Quotes::default();
    quotes.next("alice").unwrap();
    assert_eq!(quotes.shown("alice", 5).id, 6);
    assert_eq!(quotes.shown("bob", 5).id, 5);
    assert_eq!(quotes.shown("alice", 6).id, 7, "still a step ahead");
}

#[test]
fn a_pinned_quote_stays_through_every_hour_until_unpinned() {
    let mut quotes = Quotes::default();
    quotes.pin("alice", 3).unwrap();
    for hour in [4, 5, 100] {
        let shown = quotes.shown("alice", hour);
        assert_eq!(shown.id, 3);
        assert!(shown.pinned);
    }
    assert_eq!(
        quotes.next("alice").unwrap_err().message,
        "Unpin this quote to see another"
    );
    assert_eq!(
        quotes.pin("alice", 4).unwrap_err().message,
        "This quote is already pinned"
    );
    quotes.unpin("alice").unwrap();
    assert_eq!(quotes.shown("alice", 100).id, 100 % QUOTES.len());
    assert_eq!(
        quotes.unpin("alice").unwrap_err().message,
        "No quote is pinned"
    );
}

#[tokio::test]
async fn a_skip_is_yours_alone_and_announced_for_your_other_browser() {
    let quote = quote().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let before = quote.get(&bob, "/api/quote").await.body;
    let mut events = quote.listen(&alice).await;

    let skipped = quote.write(&alice, "POST", "/api/quote/next", None).await;
    assert_eq!(skipped.status, 200, "{}", skipped.body);
    assert_ne!(skipped.body["id"], before["id"]);
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    assert_eq!(quote.get(&bob, "/api/quote").await.body, before);
}

#[tokio::test]
async fn pin_and_unpin_through_the_shell() {
    let quote = quote().await;
    let alice = person("u-alice", "Alice");
    let pinned = quote.write(&alice, "PUT", "/api/quote/pin", None).await;
    assert_eq!(pinned.body["pinned"], true);
    let refused = quote.write(&alice, "POST", "/api/quote/next", None).await;
    assert_eq!(refused.status, 409);
    let unpinned = quote.write(&alice, "DELETE", "/api/quote/pin", None).await;
    assert_eq!(unpinned.body["pinned"], false);

    let Envelope::Records(table) = quote.fallback(&alice).await else {
        panic!("the quote's fallback is a table");
    };
    assert_eq!(table.rows[0]["quote"], unpinned.body["text"]);
    assert_eq!(table.rows[0]["by"], unpinned.body["by"]);
}
