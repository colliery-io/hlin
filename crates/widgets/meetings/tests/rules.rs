//! Today's meetings, called the way the shell calls them.

use chrono::{DateTime, NaiveDate, Utc};
use hlin_widget_meetings::{Meetings, PANEL, api, day, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{ModuleFiles, Site};
use serde_json::json;

/// Ten past noon on a Friday: the stand-up is over, the afternoon is not.
fn noon() -> DateTime<Utc> {
    "2026-09-25T12:10:00Z".parse().unwrap()
}

async fn meetings() -> Running {
    testing::start(widget(), Meetings::at(noon()), api()).await
}

/// The id of today's first meeting that starts after `noon()`.
fn later() -> String {
    day(noon().date_naive())
        .into_iter()
        .find(|meeting| meeting.starts > noon())
        .expect("something on this afternoon")
        .id
}

fn answer(id: &str) -> String {
    format!("/api/meetings/{id}/answer")
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_todays_meetings_as_a_table() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(!panel.pushed, "nobody else's answers are news to you");
}

#[test]
fn a_day_is_the_same_for_everybody_and_starts_with_a_stand_up() {
    let friday = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    assert_eq!(day(friday), day(friday));
    let meetings = day(friday);
    assert_eq!(meetings[0].title, "Stand-up");
    assert_eq!(meetings[0].id, "2026-09-25-0930");
    assert!(
        meetings
            .windows(2)
            .all(|pair| pair[0].ends <= pair[1].starts)
    );
    assert!(meetings.iter().all(|meeting| !meeting.with.is_empty()));
    // Days differ.
    let saturday = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
    let titles = |date| day(date).into_iter().map(|m| m.title).collect::<Vec<_>>();
    let week: Vec<_> = (0..7)
        .map(|n| titles(saturday + chrono::Duration::days(n)))
        .collect();
    assert!(week.iter().any(|titles| *titles != week[0]));
}

#[tokio::test]
async fn everybody_sees_todays_meetings() {
    let meetings = meetings().await;
    let seen = meetings
        .get(&person("u-alice", "Alice"), "/api/meetings")
        .await;
    assert_eq!(seen.status, 200);
    let ids: Vec<_> = seen
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].clone())
        .collect();
    let today: Vec<_> = day(noon().date_naive())
        .into_iter()
        .map(|m| json!(m.id))
        .collect();
    assert_eq!(ids, today);
    assert!(seen.body[0]["answer"].is_null(), "nobody has answered yet");
}

#[tokio::test]
async fn an_answer_is_yours_alone_and_can_be_changed() {
    let meetings = meetings().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let id = later();

    let going = meetings
        .write(
            &alice,
            "PUT",
            &answer(&id),
            Some(json!({ "answer": "going" })),
        )
        .await;
    assert_eq!(going.status, 200, "{}", going.body);
    meetings
        .write(
            &alice,
            "PUT",
            &answer(&id),
            Some(json!({ "answer": "maybe" })),
        )
        .await;

    let find = |body: &serde_json::Value| {
        body.as_array()
            .unwrap()
            .iter()
            .find(|meeting| meeting["id"] == json!(id))
            .unwrap()["answer"]
            .clone()
    };
    assert_eq!(
        find(&meetings.get(&alice, "/api/meetings").await.body),
        "maybe"
    );
    assert!(find(&meetings.get(&bob, "/api/meetings").await.body).is_null());
}

#[tokio::test]
async fn a_meeting_that_is_over_cannot_be_answered() {
    let meetings = meetings().await;
    let refused = meetings
        .write(
            &person("u-alice", "Alice"),
            "PUT",
            &answer("2026-09-25-0930"),
            Some(json!({ "answer": "going" })),
        )
        .await;
    assert_eq!(refused.status, 409);
    assert_eq!(refused.body, json!({ "message": "That meeting is over" }));
}

#[tokio::test]
async fn only_todays_meetings_can_be_answered() {
    let meetings = meetings().await;
    let refused = meetings
        .write(
            &person("u-alice", "Alice"),
            "PUT",
            &answer("2026-09-26-0930"),
            Some(json!({ "answer": "going" })),
        )
        .await;
    assert_eq!(refused.status, 404);
}

#[tokio::test]
async fn an_answer_is_going_maybe_or_declined() {
    let meetings = meetings().await;
    let alice = person("u-alice", "Alice");
    for body in [json!({ "answer": "perhaps" }), json!({ "going": true })] {
        let refused = meetings
            .write(&alice, "PUT", &answer(&later()), Some(body.clone()))
            .await;
        assert_eq!(refused.status, 400, "{body}");
    }
}

#[tokio::test]
async fn an_answer_is_announced_on_the_event_stream() {
    let meetings = meetings().await;
    let alice = person("u-alice", "Alice");
    let mut events = meetings.listen(&alice).await;
    meetings
        .write(
            &alice,
            "PUT",
            &answer(&later()),
            Some(json!({ "answer": "declined" })),
        )
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_is_todays_meetings_with_your_answers() {
    let meetings = meetings().await;
    let alice = person("u-alice", "Alice");
    let id = later();
    meetings
        .write(
            &alice,
            "PUT",
            &answer(&id),
            Some(json!({ "answer": "going" })),
        )
        .await;

    let Envelope::Records(table) = meetings.fallback(&alice).await else {
        panic!("the meetings' fallback is records");
    };
    let today = day(noon().date_naive());
    assert_eq!(table.rows.len(), today.len());
    let at = today.iter().position(|meeting| meeting.id == id).unwrap();
    assert_eq!(table.rows[at]["answer"], "Going");
    assert_eq!(table.rows[0]["title"], "Stand-up");
}

// -- Its own UI's `/api/` and Hlin's `/hlin/api/`, over the same handlers ----

/// Today's meetings, laid out as the demo runs it: its own `/api/` as a fixed local
/// user, Hlin's surface under `/hlin`.
async fn meetings_with_its_own_ui() -> Running {
    let layout = Site {
        local_user: Some("Local User".to_string()),
        ..Site::default()
    };
    testing::start_site(
        widget(),
        Meetings::at(noon()),
        api(),
        ModuleFiles::none(),
        layout,
    )
    .await
}

#[tokio::test]
async fn an_answer_on_its_own_page_is_the_local_users_and_nobody_elses() {
    let meetings = meetings_with_its_own_ui().await;
    let alice = person("u-alice", "Alice");
    let id = later();

    let answered = meetings
        .own(
            "PUT",
            &answer(&id),
            Some("k1"),
            Some(json!({ "answer": "going" })),
        )
        .await;
    assert!((200..300).contains(&answered.status), "{}", answered.body);
    meetings
        .write(
            &alice,
            "PUT",
            &answer(&id),
            Some(json!({ "answer": "maybe" })),
        )
        .await;

    let find = |today: &serde_json::Value| {
        today
            .as_array()
            .unwrap()
            .iter()
            .find(|meeting| meeting["id"] == id.as_str())
            .unwrap()["answer"]
            .clone()
    };
    let own = meetings.own("GET", "/api/meetings", None, None).await;
    let hlin = meetings.get(&alice, "/api/meetings").await;
    assert_eq!(find(&own.body), "going");
    assert_eq!(find(&hlin.body), "maybe");
}
