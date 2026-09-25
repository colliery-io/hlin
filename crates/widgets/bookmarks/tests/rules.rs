//! The bookmarks, called the way the shell calls them.

use hlin_widget_bookmarks::{Bookmarks, LONGEST_TITLE, MOST, PANEL, api, host, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn bookmarks() -> Running {
    testing::start(widget(), Bookmarks::seed(), api()).await
}

async fn empty() -> Running {
    testing::start(widget(), Bookmarks::default(), api()).await
}

fn titles(shelf: &Value) -> Vec<String> {
    shelf["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|link| link["title"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_links_as_a_pushed_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(panel.pushed);
}

#[test]
fn a_host_is_found_only_in_a_web_link() {
    assert_eq!(host("https://example.com/a?b#c"), Some("example.com"));
    assert_eq!(host("http://me@example.com:8080"), Some("example.com"));
    assert_eq!(host("ftp://example.com"), None);
    assert_eq!(host("https://"), None);
    assert_eq!(host("example.com"), None);
}

#[tokio::test]
async fn a_link_one_person_adds_is_on_everybodys_list_newest_first() {
    let bookmarks = bookmarks().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let added = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://example.com/runbook", "title": "Runbook" })),
        )
        .await;
    assert_eq!(added.status, 201, "{}", added.body);

    let bobs = bookmarks.get(&bob, "/api/bookmarks").await.body;
    assert_eq!(titles(&bobs)[0], "Runbook");
    assert_eq!(bobs["links"][0]["added_by"], "Alice");
    assert_eq!(bobs["links"][0]["host"], "example.com");
    assert_eq!(bobs["links"][0]["removable"], false);
    assert_eq!(bobs["room"], MOST - 4);
}

#[tokio::test]
async fn without_a_title_the_host_is_the_title() {
    let bookmarks = empty().await;
    let alice = person("u-alice", "Alice");
    let added = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://status.example.org/", "title": "  " })),
        )
        .await;
    assert_eq!(titles(&added.body), vec!["status.example.org"]);
}

#[tokio::test]
async fn only_a_web_link_with_a_sensible_title_is_added() {
    let bookmarks = empty().await;
    let alice = person("u-alice", "Alice");

    for url in ["javascript:alert(1)", "example.com", "https://exa mple.com"] {
        let refused = bookmarks
            .write(
                &alice,
                "POST",
                "/api/bookmarks",
                Some(json!({ "url": url })),
            )
            .await;
        assert_eq!(refused.status, 400, "{url}");
    }

    let long = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://example.com", "title": "x".repeat(LONGEST_TITLE + 1) })),
        )
        .await;
    assert_eq!(long.status, 400);
}

#[tokio::test]
async fn the_same_link_twice_is_refused_however_it_is_written() {
    let bookmarks = bookmarks().await;
    let alice = person("u-alice", "Alice");

    let again = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "HTTPS://BOOK.LEPTOS.DEV" })),
        )
        .await;
    assert_eq!(again.status, 409);
    assert_eq!(
        again.body,
        json!({ "message": "That link is already here" })
    );
}

#[tokio::test]
async fn the_list_holds_twenty_at_most() {
    let bookmarks = empty().await;
    let alice = person("u-alice", "Alice");
    for n in 0..MOST {
        let added = bookmarks
            .write(
                &alice,
                "POST",
                "/api/bookmarks",
                Some(json!({ "url": format!("https://example.com/{n}") })),
            )
            .await;
        assert_eq!(added.status, 201);
    }

    let full = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://example.com/one-more" })),
        )
        .await;
    assert_eq!(full.status, 409);
    assert_eq!(
        full.body,
        json!({ "message": "20 links is the most; remove one first" })
    );
}

#[tokio::test]
async fn a_link_is_removed_by_whoever_added_it_and_the_teams_by_anybody() {
    let bookmarks = bookmarks().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let added = bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://example.com", "title": "Alice's" })),
        )
        .await;
    let id = added.body["links"][0]["id"].as_u64().unwrap();

    let refused = bookmarks
        .write(&bob, "DELETE", &format!("/api/bookmarks/{id}"), None)
        .await;
    assert_eq!(refused.status, 403);
    assert_eq!(
        refused.body,
        json!({ "message": "Only Alice can remove this link" })
    );

    let teams = bookmarks.get(&bob, "/api/bookmarks").await.body["links"][1]["id"]
        .as_u64()
        .unwrap();
    let removed = bookmarks
        .write(&bob, "DELETE", &format!("/api/bookmarks/{teams}"), None)
        .await;
    assert_eq!(removed.status, 200);

    let own = bookmarks
        .write(&alice, "DELETE", &format!("/api/bookmarks/{id}"), None)
        .await;
    assert_eq!(own.status, 200);
    assert_eq!(titles(&own.body).len(), 2);

    let gone = bookmarks
        .write(&alice, "DELETE", &format!("/api/bookmarks/{id}"), None)
        .await;
    assert_eq!(gone.status, 404);
}

#[tokio::test]
async fn an_added_link_is_announced_on_the_event_stream() {
    let bookmarks = bookmarks().await;
    let alice = person("u-alice", "Alice");
    let mut events = bookmarks.listen(&alice).await;

    bookmarks
        .write(
            &alice,
            "POST",
            "/api/bookmarks",
            Some(json!({ "url": "https://example.com" })),
        )
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_the_links_newest_first() {
    let bookmarks = bookmarks().await;
    let alice = person("u-alice", "Alice");

    let Envelope::Records(table) = bookmarks.fallback(&alice).await else {
        panic!("the bookmarks' fallback is a table");
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0]["title"], "The Leptos book");
    assert_eq!(table.rows[0]["added_by"], "The team");
}
