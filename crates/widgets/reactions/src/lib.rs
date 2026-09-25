//! Widget 14 of twenty ([[HLIN-I-0012]]): emoji counters.
//!
//! A row of emoji, each with a count of the people who reacted with it.
//! Shared: everybody sees the same counts, and a reaction is announced so
//! every open panel fetches again. What differs per person is which of the
//! reactions are theirs.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/reactions` | Every emoji, its count, a few names, and whether it is yours |
//! | `PUT` | `/api/reactions/{emoji}` | React with it |
//! | `DELETE` | `/api/reactions/{emoji}` | Take your reaction back |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the counts as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may react, once per emoji: reacting again with the
//!   same one is refused, not counted twice.
//! - At most [`MOST_EACH`] different reactions per person, so a count is
//!   people who meant it rather than someone clicking along the row.
//! - Only the emoji this widget offers ([`EMOJI`]).
//! - Counted by the stable id (`sub`), never the name.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::Serialize;
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "reactions";

/// The emoji on offer: id, emoji, and what a screen reader says.
pub const EMOJI: &[(&str, &str, &str)] = &[
    ("thumbs-up", "👍", "Thumbs up"),
    ("heart", "❤️", "Heart"),
    ("party", "🎉", "Party"),
    ("laugh", "😂", "Laughing"),
    ("eyes", "👀", "Eyes"),
    ("rocket", "🚀", "Rocket"),
];

/// How many different reactions one person may have at once.
pub const MOST_EACH: usize = 3;

/// How many names a reaction lists before it says "and N more".
pub const NAMES_SHOWN: usize = 3;

/// Everybody's reactions: for each emoji id, who reacted, by `sub`, with the
/// name they had when they did.
#[derive(Debug, Clone, Default)]
pub struct Reactions {
    by: BTreeMap<&'static str, BTreeMap<String, String>>,
}

/// The reactions as one person sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    /// Every emoji, in the widget's order.
    pub reactions: Vec<Reaction>,
    /// How many more the viewer may add.
    pub left: usize,
}

/// One emoji and who reacted with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reaction {
    /// The emoji's id.
    pub id: &'static str,
    /// The emoji.
    pub emoji: &'static str,
    /// What it is called.
    pub label: &'static str,
    /// How many reacted with it.
    pub count: usize,
    /// The first few of them, by name, in name order.
    pub names: Vec<String>,
    /// Whether the viewer is one of them.
    pub mine: bool,
}

fn emoji(id: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    EMOJI.iter().find(|(known, _, _)| *known == id)
}

impl Reactions {
    fn mine(&self, claims: &Claims) -> usize {
        self.by
            .values()
            .filter(|who| who.contains_key(&claims.sub))
            .count()
    }

    /// The reactions as `claims` sees them.
    pub fn board(&self, claims: &Claims) -> Board {
        Board {
            reactions: EMOJI
                .iter()
                .map(|(id, emoji, label)| {
                    let who = self.by.get(id);
                    let mut names: Vec<String> = who
                        .map(|who| who.values().cloned().collect())
                        .unwrap_or_default();
                    names.sort();
                    names.truncate(NAMES_SHOWN);
                    Reaction {
                        id,
                        emoji,
                        label,
                        count: who.map_or(0, BTreeMap::len),
                        names,
                        mine: who.is_some_and(|who| who.contains_key(&claims.sub)),
                    }
                })
                .collect(),
            left: MOST_EACH.saturating_sub(self.mine(claims)),
        }
    }

    /// React with `id` as `claims`.
    pub fn react(&mut self, claims: &Claims, id: &str) -> Result<(), Refusal> {
        let Some((id, _, _)) = emoji(id) else {
            return Err(Refusal::not_found("There is no such reaction"));
        };
        if self
            .by
            .get(id)
            .is_some_and(|who| who.contains_key(&claims.sub))
        {
            return Err(Refusal::conflict("You have already reacted with that"));
        }
        if self.mine(claims) >= MOST_EACH {
            return Err(Refusal::conflict(format!(
                "{MOST_EACH} reactions each is the most; take one back first"
            )));
        }
        self.by
            .entry(id)
            .or_default()
            .insert(claims.sub.clone(), display_name(claims));
        Ok(())
    }

    /// Take `claims`'s reaction with `id` back.
    pub fn take_back(&mut self, claims: &Claims, id: &str) -> Result<(), Refusal> {
        let removed = emoji(id)
            .and_then(|(id, _, _)| self.by.get_mut(id))
            .and_then(|who| who.remove(&claims.sub));
        match removed {
            Some(_) => Ok(()),
            None => Err(Refusal::not_found("You have not reacted with that")),
        }
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Reactions> {
    Widget {
        panel: PANEL,
        name: "Reactions",
        icon: "smile",
        title: "Reactions",
        description: "Emoji counters everybody shares",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |reactions, claims| Envelope::Records(as_table(&reactions.board(claims))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The counts as a table, for the shell to draw where the module is not.
fn as_table(board: &Board) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("reaction", "Reaction", ColumnType::String),
            column("count", "People", ColumnType::Number),
        ],
        rows: board
            .reactions
            .iter()
            .map(|reaction| {
                let row = json!({
                    "reaction": format!("{} {}", reaction.emoji, reaction.label),
                    "count": reaction.count,
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Reactions>> {
    Router::new()
        .route("/api/reactions", get(read))
        .route("/api/reactions/{emoji}", put(react).delete(take_back))
}

async fn read(State(platform): State<Platform<Reactions>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|reactions| reactions.board(&claims))).into_response()
}

async fn react(
    State(platform): State<Platform<Reactions>>,
    Path(emoji): Path<String>,
    write: Write,
) -> Response {
    platform.write(&write, |reactions, claims| {
        reactions.react(claims, &emoji)?;
        Ok(Reply::ok(reactions.board(claims)))
    })
}

async fn take_back(
    State(platform): State<Platform<Reactions>>,
    Path(emoji): Path<String>,
    write: Write,
) -> Response {
    platform.write(&write, |reactions, claims| {
        reactions.take_back(claims, &emoji)?;
        Ok(Reply::ok(reactions.board(claims)))
    })
}
