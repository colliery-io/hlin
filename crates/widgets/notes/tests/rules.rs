//! The note, called the way the shell calls it.

use hlin_widget_notes::{LONGEST, Note, PANEL, api, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{ModuleFiles, Site};
use serde_json::json;

async fn notes() -> Running {
    testing::start(widget(), Note::default(), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_note_as_a_table_and_the_note_is_shared() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(panel.pushed, "one person's edit is everyone's note");
}

#[tokio::test]
async fn anyone_may_edit_it_and_everyone_reads_the_same_note() {
    let notes = notes().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let edited = notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "Bring cake", "revision": 0 })),
        )
        .await;
    assert_eq!(edited.status, 200, "{}", edited.body);

    let seen = notes.get(&bob, "/api/note").await;
    assert_eq!(
        seen.body,
        json!({ "text": "Bring cake", "revision": 1, "by": "Alice" })
    );
}

#[tokio::test]
async fn an_edit_written_against_an_old_note_is_refused_not_lost() {
    let notes = notes().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    // Both open the note at revision 0; Alice saves first.
    notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "Alice's words", "revision": 0 })),
        )
        .await;
    let refused = notes
        .write(
            &bob,
            "PUT",
            "/api/note",
            Some(json!({ "text": "Bob's words", "revision": 0 })),
        )
        .await;

    assert_eq!(refused.status, 409);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .starts_with("Someone else changed the note")
    );
    assert_eq!(
        notes.get(&bob, "/api/note").await.body["text"],
        "Alice's words"
    );
}

#[tokio::test]
async fn a_note_is_short() {
    let notes = notes().await;
    let alice = person("u-alice", "Alice");

    let longest = "x".repeat(LONGEST);
    let fits = notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": longest, "revision": 0 })),
        )
        .await;
    assert_eq!(fits.status, 200);

    let refused = notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "y".repeat(LONGEST + 1), "revision": 1 })),
        )
        .await;
    assert_eq!(refused.status, 400);
    assert_eq!(
        refused.body,
        json!({ "message": "A note is at most 500 characters" })
    );
}

#[tokio::test]
async fn it_may_be_wiped_clean_and_an_edit_that_changes_nothing_moves_nothing() {
    let notes = notes().await;
    let alice = person("u-alice", "Alice");

    let wiped = notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "", "revision": 0 })),
        )
        .await;
    assert_eq!(wiped.body["text"], "");
    assert_eq!(wiped.body["revision"], 1);

    let same = notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "  ", "revision": 1 })),
        )
        .await;
    assert_eq!(same.status, 200);
    assert_eq!(same.body["revision"], 1, "nobody else's draft goes stale");
}

#[tokio::test]
async fn an_edit_is_the_whole_text_and_its_revision() {
    let notes = notes().await;
    let alice = person("u-alice", "Alice");
    for body in [json!({ "text": "no revision" }), json!({ "revision": 0 })] {
        let refused = notes
            .write(&alice, "PUT", "/api/note", Some(body.clone()))
            .await;
        assert_eq!(refused.status, 400, "{body}");
    }
}

#[tokio::test]
async fn an_edit_is_announced_and_the_fallback_shows_it() {
    let notes = notes().await;
    let alice = person("u-alice", "Alice");
    let mut events = notes.listen(&alice).await;

    notes
        .write(
            &alice,
            "PUT",
            "/api/note",
            Some(json!({ "text": "Ship it", "revision": 0 })),
        )
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Records(table) = notes.fallback(&alice).await else {
        panic!("the note's fallback is a table");
    };
    assert_eq!(table.rows[0]["note"], "Ship it");
    assert_eq!(table.rows[0]["by"], "Alice");
}

// -- Its own UI (HLIN-I-0013) ---------------------------------------------

/// The note as its own UI reaches it: `/api/` from the root, as the demo's
/// local user, over the same handlers as Hlin's `/hlin/api/`.
async fn notes_with_its_own_ui() -> Running {
    let layout = Site {
        local_user: Some("Local User".to_string()),
        ..Site::default()
    };
    testing::start_site(
        widget(),
        Note::default(),
        api(),
        ModuleFiles::none(),
        layout,
    )
    .await
}

#[tokio::test]
async fn its_own_ui_edits_the_same_note_hlin_reads() {
    let notes = notes_with_its_own_ui().await;
    let edited = notes
        .own(
            "PUT",
            "/api/note",
            Some(&testing::fresh_key()),
            Some(json!({ "text": "From its own page", "revision": 0 })),
        )
        .await;
    assert_eq!(edited.status, 200, "{}", edited.body);

    let seen = notes.get(&person("u-bob", "Bob"), "/api/note").await;
    assert_eq!(
        seen.body,
        json!({ "text": "From its own page", "revision": 1, "by": "Local User" })
    );
}
