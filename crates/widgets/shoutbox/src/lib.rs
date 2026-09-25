//! Widget 13 of twenty ([[HLIN-I-0012]]): a tiny chat.
//!
//! Shared: one room, everybody in it. Every shout is announced, so every
//! open shoutbox fetches again and shows it, without a reload.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/shouts` | The latest shouts, oldest first, yours marked |
//! | `POST` | `/api/shouts` | `{ "text": "..." }`: say something |
//! | `DELETE` | `/api/shouts/{id}` | Take back something you said |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the latest shouts as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may shout: something, and at most [`LONGEST`]
//!   characters, with the space either side trimmed.
//! - Saying exactly the same thing twice in a row is refused, which is
//!   almost always a double click.
//! - The room remembers the last [`KEPT`] shouts; older ones go.
//! - A shout can be taken back only by whoever said it, known by the stable
//!   id (`sub`), not the name.

use std::collections::VecDeque;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use chrono::{DateTime, Utc};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "shoutbox";

/// The longest shout, in characters.
pub const LONGEST: usize = 280;

/// How many shouts the room remembers.
pub const KEPT: usize = 50;

/// The room.
#[derive(Debug, Clone, Default)]
pub struct Shoutbox {
    shouts: VecDeque<Shout>,
    next: u64,
}

/// One shout, as the room keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shout {
    id: u64,
    sub: String,
    name: String,
    text: String,
    at: DateTime<Utc>,
}

/// The room as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Room {
    /// The shouts, oldest first.
    pub shouts: Vec<Seen>,
}

/// A shout, as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Seen {
    /// Its id, for taking it back.
    pub id: u64,
    /// Who said it, as they are called.
    pub name: String,
    /// What they said.
    pub text: String,
    /// When, in milliseconds since the epoch, for the browser to show on its
    /// own clock.
    pub at_ms: i64,
    /// Whether the viewer said it, and so may take it back.
    pub mine: bool,
}

/// What a shout asks for.
#[derive(Debug, Deserialize)]
pub struct Say {
    /// What to say.
    pub text: String,
}

impl Shoutbox {
    /// The room as `claims` sees it.
    pub fn room(&self, claims: &Claims) -> Room {
        Room {
            shouts: self
                .shouts
                .iter()
                .map(|shout| Seen {
                    id: shout.id,
                    name: shout.name.clone(),
                    text: shout.text.clone(),
                    at_ms: shout.at.timestamp_millis(),
                    mine: shout.sub == claims.sub,
                })
                .collect(),
        }
    }

    /// Say `text` as `claims` at `at`, if the rules allow. Answers the
    /// shout's id.
    pub fn say(&mut self, claims: &Claims, text: &str, at: DateTime<Utc>) -> Result<u64, Refusal> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Refusal::bad_request("Say something first"));
        }
        if text.chars().count() > LONGEST {
            return Err(Refusal::bad_request(format!(
                "A shout is at most {LONGEST} characters"
            )));
        }
        if self
            .shouts
            .back()
            .is_some_and(|last| last.sub == claims.sub && last.text == text)
        {
            return Err(Refusal::conflict("You just said that"));
        }
        self.next += 1;
        self.shouts.push_back(Shout {
            id: self.next,
            sub: claims.sub.clone(),
            name: display_name(claims),
            text: text.to_string(),
            at,
        });
        while self.shouts.len() > KEPT {
            self.shouts.pop_front();
        }
        Ok(self.next)
    }

    /// Take back shout `id`, if `claims` said it.
    pub fn take_back(&mut self, claims: &Claims, id: u64) -> Result<(), Refusal> {
        let Some(at) = self.shouts.iter().position(|shout| shout.id == id) else {
            return Err(Refusal::not_found("There is no such shout"));
        };
        if self.shouts[at].sub != claims.sub {
            return Err(Refusal::forbidden(
                "Only whoever said it can take a shout back",
            ));
        }
        self.shouts.remove(at);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Shoutbox> {
    Widget {
        panel: PANEL,
        name: "Shoutbox",
        icon: "message",
        title: "Shoutbox",
        description: "A tiny chat for everybody on the surface",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |shoutbox, _| Envelope::Records(as_table(shoutbox)),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The latest shouts, newest first, for the shell to draw where the module
/// is not.
fn as_table(shoutbox: &Shoutbox) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("at", "When", ColumnType::Timestamp),
            column("name", "Who", ColumnType::String),
            column("text", "Said", ColumnType::String),
        ],
        rows: shoutbox
            .shouts
            .iter()
            .rev()
            .map(|shout| {
                let row = json!({
                    "at": shout.at.to_rfc3339(),
                    "name": shout.name,
                    "text": shout.text,
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Shoutbox>> {
    Router::new()
        .route("/api/shouts", get(read).post(say))
        .route("/api/shouts/{id}", delete(take_back))
}

async fn read(State(platform): State<Platform<Shoutbox>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|shoutbox| shoutbox.room(&claims))).into_response()
}

async fn say(State(platform): State<Platform<Shoutbox>>, write: Write) -> Response {
    platform.write(&write, |shoutbox, claims| {
        let asked: Say = write.json("A shout is { \"text\": \"...\" }")?;
        shoutbox.say(claims, &asked.text, Utc::now())?;
        Ok(Reply::created(shoutbox.room(claims)))
    })
}

async fn take_back(
    State(platform): State<Platform<Shoutbox>>,
    Path(id): Path<u64>,
    write: Write,
) -> Response {
    platform.write(&write, |shoutbox, claims| {
        shoutbox.take_back(claims, id)?;
        Ok(Reply::ok(shoutbox.room(claims)))
    })
}
