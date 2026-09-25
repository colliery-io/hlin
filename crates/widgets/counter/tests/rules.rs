//! The counter, called the way the shell calls it.

use hlin_widget_counter::{Counter, PANEL, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::json;

async fn counter() -> Running {
    testing::start(widget(), Counter::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_number_as_a_stat() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("stat"));
    assert_eq!(panel.envelope.as_deref(), Some("scalar.v1"));
    assert!(panel.pushed, "one person's bump is everyone's number");
}

#[tokio::test]
async fn anyone_may_bump_it_up_and_everyone_sees_the_same_number() {
    let counter = counter().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let bumped = counter
        .write(
            &alice,
            "POST",
            "/api/counter/bump",
            Some(json!({ "by": 1 })),
        )
        .await;
    assert_eq!(bumped.status, 200, "{}", bumped.body);
    counter
        .write(&bob, "POST", "/api/counter/bump", Some(json!({ "by": 1 })))
        .await;

    let seen = counter.get(&alice, "/api/counter").await;
    assert_eq!(seen.body["value"], 2);
    assert_eq!(seen.body["last"], json!({ "name": "Bob", "by": 1 }));
}

#[tokio::test]
async fn it_never_goes_below_zero() {
    let counter = counter().await;
    let alice = person("u-alice", "Alice");

    let refused = counter
        .write(
            &alice,
            "POST",
            "/api/counter/bump",
            Some(json!({ "by": -1 })),
        )
        .await;
    assert_eq!(refused.status, 409);
    assert_eq!(
        refused.body,
        json!({ "message": "The counter is already at zero" })
    );
}

#[tokio::test]
async fn a_bump_is_by_one_and_nothing_else() {
    let counter = counter().await;
    let alice = person("u-alice", "Alice");

    for body in [
        json!({ "by": 5 }),
        json!({ "by": 0 }),
        json!({ "up": true }),
    ] {
        let refused = counter
            .write(&alice, "POST", "/api/counter/bump", Some(body.clone()))
            .await;
        assert_eq!(refused.status, 400, "{body}");
    }
    assert_eq!(counter.get(&alice, "/api/counter").await.body["value"], 0);
}

#[tokio::test]
async fn a_retried_bump_counts_once() {
    let counter = counter().await;
    let alice = person("u-alice", "Alice");
    let body = Some(json!({ "by": 1 }));

    let first = counter
        .write_keyed(&alice, "POST", "/api/counter/bump", body.clone(), "k1")
        .await;
    let again = counter
        .write_keyed(&alice, "POST", "/api/counter/bump", body, "k1")
        .await;

    assert_eq!(again.body, first.body);
    assert!(again.replayed);
    assert_eq!(counter.get(&alice, "/api/counter").await.body["value"], 1);
}

#[tokio::test]
async fn a_bump_is_announced_on_the_event_stream() {
    let counter = counter().await;
    let alice = person("u-alice", "Alice");
    let mut events = counter.listen(&alice).await;

    counter
        .write(
            &alice,
            "POST",
            "/api/counter/bump",
            Some(json!({ "by": 1 })),
        )
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_says_the_number_and_who_moved_it() {
    let counter = counter().await;
    let alice = person("u-alice", "Alice");
    counter
        .write(
            &alice,
            "POST",
            "/api/counter/bump",
            Some(json!({ "by": 1 })),
        )
        .await;

    let Envelope::Scalar(stat) = counter.fallback(&alice).await else {
        panic!("the counter's fallback is a scalar");
    };
    assert_eq!(stat.value, 1);
    assert_eq!(stat.label.as_deref(), Some("Last bumped by Alice"));
}
