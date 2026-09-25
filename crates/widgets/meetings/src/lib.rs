//! Widget 20 of twenty ([[HLIN-I-0012]]): today's meetings.
//!
//! The meetings are synthetic: a day's are a pure function of its date
//! ([`day`]), the same for everybody, so there is no calendar to keep. What a
//! person keeps is their answer to each (going, maybe, declined), which is
//! theirs alone. Not shared: nobody sees anybody else's answers. Answering is
//! still announced, as every widget's writes are, so the same person's other
//! browser agrees; the panel does not declare `pushed`.
//!
//! Times are instants (UTC on the wire); the module shows them in the
//! browser's own time zone. "Today" is the UTC day, which is what a server
//! with no idea where its viewer is can honestly mean.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/meetings` | Today's meetings, with your answers |
//! | `PUT` | `/api/meetings/{id}/answer` | `{ "answer": "going" \| "maybe" \| "declined" }` |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: today's meetings and your answers, as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in sees today's meetings.
//! - You may answer any of today's meetings that has not ended, and change
//!   your answer; one that is over keeps whatever you said before it ended.
//! - An answer is going, maybe or declined. Nothing else.
//! - Your answers are yours: nobody else sees them or changes them.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "meetings";

/// When meetings may start, UTC, and how long each is: hour, minute, minutes.
const SLOTS: &[(u32, u32, i64)] = &[
    (9, 30, 15),
    (10, 30, 45),
    (12, 0, 60),
    (14, 0, 30),
    (15, 30, 50),
    (16, 30, 25),
];

/// What a meeting may be about, and where.
const KINDS: &[(&str, &str)] = &[
    ("Design review", "Atrium"),
    ("1:1", "Nook"),
    ("Incident retro", "Harbour"),
    ("Sprint planning", "Fjord"),
    ("Lunch and learn", "Canteen"),
    ("Hiring sync", "Nook"),
    ("Customer call", "Booth 2"),
    ("Demo prep", "Atrium"),
    ("Roadmap", "Harbour"),
];

/// Who might be there.
const PEOPLE: &[&str] = &["Ada", "Grace", "Linus", "Margaret", "Ken", "Barbara"];

/// Your answer to a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    /// You will be there.
    Going,
    /// You might.
    Maybe,
    /// You will not.
    Declined,
}

impl Answer {
    fn words(self) -> &'static str {
        match self {
            Self::Going => "Going",
            Self::Maybe => "Maybe",
            Self::Declined => "Declined",
        }
    }
}

/// One meeting, as the person asking sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Meeting {
    /// Its id: the day and the time, `2026-09-25-0930`.
    pub id: String,
    /// What it is.
    pub title: String,
    /// Where.
    pub room: String,
    /// When it starts.
    pub starts: DateTime<Utc>,
    /// When it ends.
    pub ends: DateTime<Utc>,
    /// Who else is invited.
    pub with: Vec<String>,
    /// Your answer, if you have given one.
    pub answer: Option<Answer>,
}

/// A day's meetings, as they would be for somebody who had answered nothing.
pub fn day(date: NaiveDate) -> Vec<Meeting> {
    let n = date.num_days_from_ce() as u64;
    SLOTS
        .iter()
        .enumerate()
        .filter_map(|(slot, &(hour, minute, minutes))| {
            let bits = mix(n * 16 + slot as u64);
            // Every day starts with a stand-up; the rest come and go.
            let (title, room) = if slot == 0 {
                ("Stand-up".to_string(), "Fjord")
            } else if bits.is_multiple_of(3) {
                return None;
            } else {
                let (title, room) = KINDS[((bits >> 8) % KINDS.len() as u64) as usize];
                (title.to_string(), room)
            };
            let starts = date.and_hms_opt(hour, minute, 0)?.and_utc();
            let first = (bits >> 16) as usize % PEOPLE.len();
            let count = 1 + (bits >> 24) as usize % 4;
            Some(Meeting {
                id: format!("{}-{hour:02}{minute:02}", date.format("%Y-%m-%d")),
                title,
                room: room.to_string(),
                starts,
                ends: starts + Duration::minutes(minutes),
                with: (0..count)
                    .map(|i| PEOPLE[(first + i) % PEOPLE.len()].to_string())
                    .collect(),
                answer: None,
            })
        })
        .collect()
}

/// A scrambling of `n`, so neighbouring days differ (SplitMix64's finish).
fn mix(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Everybody's answers, and the clock the day is read from.
#[derive(Debug, Clone, Default)]
pub struct Meetings {
    /// Answers by `sub`, then by meeting id.
    answers: BTreeMap<String, BTreeMap<String, Answer>>,
    /// A fixed time, for tests; the real clock otherwise.
    fixed: Option<DateTime<Utc>>,
}

/// What answering asks for.
#[derive(Debug, Deserialize)]
pub struct Answering {
    /// The answer.
    pub answer: Answer,
}

impl Meetings {
    /// Meetings as if it were always `now`, for tests.
    pub fn at(now: DateTime<Utc>) -> Self {
        Self {
            fixed: Some(now),
            ..Self::default()
        }
    }

    /// What time it is.
    pub fn now(&self) -> DateTime<Utc> {
        self.fixed.unwrap_or_else(Utc::now)
    }

    /// Today's meetings as `claims` sees them.
    pub fn today(&self, claims: &Claims) -> Vec<Meeting> {
        let mine = self.answers.get(&claims.sub);
        day(self.now().date_naive())
            .into_iter()
            .map(|meeting| Meeting {
                answer: mine.and_then(|mine| mine.get(&meeting.id)).copied(),
                ..meeting
            })
            .collect()
    }

    /// Answer one of today's meetings, if it has not ended.
    pub fn answer(&mut self, claims: &Claims, id: &str, answer: Answer) -> Result<(), Refusal> {
        let now = self.now();
        let Some(meeting) = day(now.date_naive())
            .into_iter()
            .find(|meeting| meeting.id == id)
        else {
            return Err(Refusal::not_found("There is no such meeting today"));
        };
        if meeting.ends <= now {
            return Err(Refusal::conflict("That meeting is over"));
        }
        self.answers
            .entry(claims.sub.clone())
            .or_default()
            .insert(meeting.id, answer);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Meetings> {
    Widget {
        panel: PANEL,
        name: "Meetings",
        icon: "calendar",
        title: "Today's meetings",
        description: "What is on today, and whether you are going",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // Nothing is written when a meeting starts or ends, but the day
            // moves on; once a minute is enough for a list of meetings.
            refresh_ms: Some(60_000),
            data: |meetings, claims| Envelope::Records(as_table(meetings, claims)),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// Today's meetings as a table, for the shell to draw where the module is not.
fn as_table(meetings: &Meetings, claims: &Claims) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("starts", "When", ColumnType::Timestamp),
            column("title", "What", ColumnType::String),
            column("room", "Where", ColumnType::String),
            column("answer", "You", ColumnType::String),
        ],
        rows: meetings
            .today(claims)
            .into_iter()
            .map(|meeting| {
                let row = json!({
                    "starts": meeting.starts.to_rfc3339(),
                    "title": meeting.title,
                    "room": meeting.room,
                    "answer": meeting.answer.map_or("", Answer::words),
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: Some(meetings.now()),
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Meetings>> {
    Router::new()
        .route("/api/meetings", get(read))
        .route("/api/meetings/{id}/answer", put(answer))
}

async fn read(State(platform): State<Platform<Meetings>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|meetings| meetings.today(&claims))).into_response()
}

async fn answer(
    State(platform): State<Platform<Meetings>>,
    Path(id): Path<String>,
    write: Write,
) -> Response {
    platform.write(&write, |meetings, claims| {
        let asked: Answering =
            write.json("An answer is { \"answer\": \"going\" | \"maybe\" | \"declined\" }")?;
        meetings.answer(claims, &id, asked.answer)?;
        Ok(Reply::ok(meetings.today(claims)))
    })
}
