//! Widget 9 of twenty ([[HLIN-I-0012]]): one column of cards.
//!
//! Shared: one column, and everybody works it. Adding, moving and finishing a
//! card are announced, so every open column fetches again.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/cards` | The column, top first |
//! | `POST` | `/api/cards` | `{ "text": "..." }`: add a card at the bottom |
//! | `POST` | `/api/cards/{id}/move` | `{ "by": -1 }` up, `{ "by": 1 }` down |
//! | `DELETE` | `/api/cards/{id}` | Take a card off: done, or not wanted |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the column as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may add a card and move any card.
//! - Only the person who added a card may take it off, so nobody's work
//!   disappears because somebody else tidied.
//! - A card says something, in at most [`LONGEST`] characters.
//! - A column holds at most [`MOST`] cards: a column that long is a backlog.
//! - A move is one place up or down, and not off either end.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use hlin_widget_support::envelope::{Column as TableColumn, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "kanban";

/// The longest a card may be, in characters.
pub const LONGEST: usize = 80;

/// The most cards a column holds.
pub const MOST: usize = 12;

/// The column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// What the column is for.
    pub title: String,
    cards: Vec<Card>,
    next: u64,
}

/// One card, as the column keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Card {
    id: u64,
    text: String,
    /// Who added it, by `sub`, which is what taking it off is decided on.
    owner: String,
    /// Who added it, as they are called.
    name: String,
}

/// The column, as somebody sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    /// What the column is for.
    pub title: String,
    /// Top first.
    pub cards: Vec<Shown>,
    /// The most there may be.
    pub most: usize,
}

/// One card, as somebody sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    /// Its id.
    pub id: u64,
    /// What it says.
    pub text: String,
    /// Who added it.
    pub by: String,
    /// Whether the person looking added it, and so may take it off.
    pub mine: bool,
}

/// What adding a card asks for.
#[derive(Debug, Deserialize)]
pub struct Add {
    /// What it says.
    pub text: String,
}

/// What moving a card asks for.
#[derive(Debug, Deserialize)]
pub struct Move {
    /// `-1` up, `1` down.
    pub by: i64,
}

impl Column {
    /// An empty column called `title`.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            cards: Vec::new(),
            next: 1,
        }
    }

    /// The demo's column, with a few cards on it already.
    pub fn seed() -> Self {
        let mut column = Self::new("This week");
        for text in [
            "Rotate the staging certificates",
            "Write up the offsite poll",
            "Try the twenty-widget surface on a phone",
        ] {
            column.push("hlin", "Hlin", text);
        }
        column
    }

    fn push(&mut self, owner: &str, name: &str, text: &str) -> u64 {
        let id = self.next;
        self.next += 1;
        self.cards.push(Card {
            id,
            text: text.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
        });
        id
    }

    fn at(&self, id: u64) -> Result<usize, Refusal> {
        self.cards
            .iter()
            .position(|card| card.id == id)
            .ok_or_else(|| Refusal::not_found("That card is not on the column any more"))
    }

    /// The column as `claims` sees it.
    pub fn board(&self, claims: &Claims) -> Board {
        Board {
            title: self.title.clone(),
            cards: self
                .cards
                .iter()
                .map(|card| Shown {
                    id: card.id,
                    text: card.text.clone(),
                    by: card.name.clone(),
                    mine: card.owner == claims.sub,
                })
                .collect(),
            most: MOST,
        }
    }

    /// Add a card at the bottom, returning its id.
    pub fn add(&mut self, claims: &Claims, text: &str) -> Result<u64, Refusal> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Refusal::bad_request("A card needs something on it"));
        }
        if text.chars().count() > LONGEST {
            return Err(Refusal::bad_request(format!(
                "A card is at most {LONGEST} characters"
            )));
        }
        if self.cards.len() >= MOST {
            return Err(Refusal::conflict(format!(
                "The column holds {MOST} cards; finish one first"
            )));
        }
        Ok(self.push(&claims.sub, &display_name(claims), text))
    }

    /// Move a card one place up (`-1`) or down (`1`).
    pub fn shift(&mut self, id: u64, by: i64) -> Result<(), Refusal> {
        let at = self.at(id)?;
        let to = match by {
            -1 => at
                .checked_sub(1)
                .ok_or_else(|| Refusal::conflict("That card is already at the top"))?,
            1 if at + 1 < self.cards.len() => at + 1,
            1 => return Err(Refusal::conflict("That card is already at the bottom")),
            _ => return Err(Refusal::bad_request("A card moves one place: by -1 or 1")),
        };
        self.cards.swap(at, to);
        Ok(())
    }

    /// Take a card off, if `claims` added it.
    pub fn remove(&mut self, claims: &Claims, id: u64) -> Result<(), Refusal> {
        let at = self.at(id)?;
        if self.cards[at].owner != claims.sub {
            return Err(Refusal::forbidden(format!(
                "Only {} can take this card off",
                self.cards[at].name
            )));
        }
        self.cards.remove(at);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Column> {
    Widget {
        panel: PANEL,
        name: "Kanban",
        icon: "columns",
        title: "This week",
        description: "One column of cards the team works through",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |column, claims| Envelope::Records(as_table(&column.board(claims))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The column as a table, for the shell to draw where the module is not.
fn as_table(board: &Board) -> Records {
    let column = |key: &str, label: &str, value_type| TableColumn {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("position", "#", ColumnType::Number),
            column("card", &board.title, ColumnType::String),
            column("by", "Added by", ColumnType::String),
        ],
        rows: board
            .cards
            .iter()
            .enumerate()
            .map(|(index, card)| {
                let row = json!({ "position": index + 1, "card": card.text, "by": card.by });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Column>> {
    Router::new()
        .route("/api/cards", get(read).post(add))
        .route("/api/cards/{id}/move", post(shift))
        .route("/api/cards/{id}", delete(remove))
}

async fn read(State(platform): State<Platform<Column>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|column| column.board(&claims))).into_response()
}

async fn add(State(platform): State<Platform<Column>>, write: Write) -> Response {
    platform.write(&write, |column, claims| {
        let asked: Add = write.json("A card is { \"text\": \"...\" }")?;
        column.add(claims, &asked.text)?;
        Ok(Reply::created(column.board(claims)))
    })
}

async fn shift(
    State(platform): State<Platform<Column>>,
    Path(id): Path<u64>,
    write: Write,
) -> Response {
    platform.write(&write, |column, claims| {
        let asked: Move = write.json("A move is { \"by\": -1 } or { \"by\": 1 }")?;
        column.shift(id, asked.by)?;
        Ok(Reply::ok(column.board(claims)))
    })
}

async fn remove(
    State(platform): State<Platform<Column>>,
    Path(id): Path<u64>,
    write: Write,
) -> Response {
    platform.write(&write, |column, claims| {
        column.remove(claims, id)?;
        Ok(Reply::ok(column.board(claims)))
    })
}
