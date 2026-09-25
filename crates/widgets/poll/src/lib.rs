//! Widget 3 of twenty ([[HLIN-I-0012]]): one question, and votes.
//!
//! Shared: everybody sees the same tally, and a vote is announced so every
//! open poll fetches again. What each person sees differs in one place, which
//! of the options is theirs, which is why a read is answered for whoever asks.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/poll` | The question, the tally, and your vote |
//! | `POST` | `/api/poll/vote` | `{ "option": id }`: vote, or change your vote |
//! | `DELETE` | `/api/poll/vote` | Take your vote back |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the tally as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may vote, once: a second vote replaces the first.
//! - Votes are counted by the stable id (`sub`), never the name, so two
//!   people called Sam are two votes.
//! - Only an option the poll offers can be voted for.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "poll";

/// The poll.
#[derive(Debug, Clone)]
pub struct Poll {
    question: String,
    options: Vec<Choice>,
    /// Each voter's choice, by `sub`.
    votes: BTreeMap<String, String>,
}

/// One option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Choice {
    /// Stable, and what a vote names.
    pub id: String,
    /// What it says.
    pub label: String,
}

/// The poll as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tally {
    /// The question.
    pub question: String,
    /// Every option, in the poll's order, with its votes.
    pub options: Vec<Counted>,
    /// Votes cast.
    pub total: usize,
    /// Which option is the asker's, if they have voted.
    pub mine: Option<String>,
}

/// An option and its votes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Counted {
    /// The option's id.
    pub id: String,
    /// What it says.
    pub label: String,
    /// How many chose it.
    pub votes: usize,
}

/// What a vote asks for.
#[derive(Debug, Deserialize)]
pub struct Vote {
    /// The option's id.
    pub option: String,
}

impl Poll {
    /// A poll asking `question`, offering `options` as `(id, label)`.
    pub fn new(question: &str, options: &[(&str, &str)]) -> Self {
        Self {
            question: question.to_string(),
            options: options
                .iter()
                .map(|(id, label)| Choice {
                    id: id.to_string(),
                    label: label.to_string(),
                })
                .collect(),
            votes: BTreeMap::new(),
        }
    }

    /// The demo's poll.
    pub fn seed() -> Self {
        Self::new(
            "Where should the team offsite be?",
            &[
                ("lisbon", "Lisbon"),
                ("kyoto", "Kyoto"),
                ("reykjavik", "Reykjavík"),
            ],
        )
    }

    /// The tally, as `claims` sees it.
    pub fn tally(&self, claims: &Claims) -> Tally {
        Tally {
            question: self.question.clone(),
            options: self
                .options
                .iter()
                .map(|choice| Counted {
                    id: choice.id.clone(),
                    label: choice.label.clone(),
                    votes: self.votes.values().filter(|id| **id == choice.id).count(),
                })
                .collect(),
            total: self.votes.len(),
            mine: self.votes.get(&claims.sub).cloned(),
        }
    }

    /// Vote, or change a vote.
    pub fn vote(&mut self, claims: &Claims, option: &str) -> Result<(), Refusal> {
        if !self.options.iter().any(|choice| choice.id == option) {
            return Err(Refusal::not_found("This poll has no such option"));
        }
        self.votes.insert(claims.sub.clone(), option.to_string());
        Ok(())
    }

    /// Take a vote back.
    pub fn unvote(&mut self, claims: &Claims) -> Result<(), Refusal> {
        self.votes
            .remove(&claims.sub)
            .map(|_| ())
            .ok_or_else(|| Refusal::not_found("You have not voted"))
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Poll> {
    Widget {
        panel: PANEL,
        name: "Poll",
        icon: "chart",
        title: "Poll",
        description: "One question, and everybody's votes",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |poll, claims| Envelope::Records(as_table(&poll.tally(claims))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The tally as a table, for the shell to draw where the module is not.
fn as_table(tally: &Tally) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("option", &tally.question, ColumnType::String),
            column("votes", "Votes", ColumnType::Number),
        ],
        rows: tally
            .options
            .iter()
            .map(|counted| {
                let row = json!({ "option": counted.label, "votes": counted.votes });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Poll>> {
    Router::new()
        .route("/api/poll", get(read))
        .route("/api/poll/vote", post(vote).delete(unvote))
}

async fn read(State(platform): State<Platform<Poll>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|poll| poll.tally(&claims))).into_response()
}

async fn vote(State(platform): State<Platform<Poll>>, write: Write) -> Response {
    platform.write(&write, |poll, claims| {
        let asked: Vote = write.json("A vote is { \"option\": id }")?;
        poll.vote(claims, &asked.option)?;
        Ok(Reply::ok(poll.tally(claims)))
    })
}

async fn unvote(State(platform): State<Platform<Poll>>, write: Write) -> Response {
    platform.write(&write, |poll, claims| {
        poll.unvote(claims)?;
        Ok(Reply::ok(poll.tally(claims)))
    })
}
