//! Widget 4 of twenty ([[HLIN-I-0012]]): one shared sticky note.
//!
//! Shared: there is one note, and everybody sees and edits the same one. An
//! edit is announced, so every open note fetches again.
//!
//! Two people editing one note at once is the case a note has to get right,
//! so an edit says which revision it was written against. One written against
//! a note somebody else has since changed is refused, in words, rather than
//! quietly throwing the other person's words away: the module keeps the draft
//! and shows the note as it now is.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/note` | The note, its revision, and who wrote it last |
//! | `PUT` | `/api/note` | `{ "text": "...", "revision": n }`: replace it |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the note as a one-row `table`.
//!
//! The rules:
//!
//! - Anyone signed in may read and edit.
//! - An edit names the revision it was written against, and is refused if
//!   that is no longer the note's.
//! - At most [`LONGEST`] characters; an empty note is allowed (a note can be
//!   wiped clean).
//! - An edit that changes nothing is accepted and changes nothing, so it does
//!   not take the revision away from somebody else mid-edit.

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "notes";

/// The longest a note may be, in characters: a sticky note, not a document.
pub const LONGEST: usize = 500;

/// The note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    /// What it says.
    pub text: String,
    /// Rises by one with every edit that changed it.
    pub revision: u64,
    /// Who changed it last; nobody, for the note the demo starts with.
    pub by: Option<String>,
}

/// What an edit asks for.
#[derive(Debug, Deserialize)]
pub struct Edit {
    /// The whole new text.
    pub text: String,
    /// The revision the editor started from.
    pub revision: u64,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            text: "Stand-up moves to 10:15 on Thursday.".to_string(),
            revision: 0,
            by: None,
        }
    }
}

impl Note {
    /// Replace the text, if the rules allow.
    pub fn edit(&mut self, claims: &Claims, edit: Edit) -> Result<(), Refusal> {
        if edit.revision != self.revision {
            return Err(Refusal::conflict(
                "Someone else changed the note while you were editing. Your text is kept; \
                 look at theirs, then save again.",
            ));
        }
        let text = edit.text.trim_end().to_string();
        if text.chars().count() > LONGEST {
            return Err(Refusal::bad_request(format!(
                "A note is at most {LONGEST} characters"
            )));
        }
        if text == self.text {
            return Ok(());
        }
        self.text = text;
        self.revision += 1;
        self.by = Some(display_name(claims));
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Note> {
    Widget {
        panel: PANEL,
        name: "Notes",
        icon: "file-text",
        title: "Sticky note",
        description: "One note the whole team shares",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |note, _| Envelope::Records(as_table(note)),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The note as a one-row table, for the shell to draw where the module is not.
fn as_table(note: &Note) -> Records {
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    let row = json!({ "note": note.text, "by": note.by.clone().unwrap_or_default() });
    Records {
        columns: vec![column("note", "Note"), column("by", "Last edited by")],
        rows: vec![row.as_object().expect("an object").clone()],
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Note>> {
    Router::new().route("/api/note", get(read).put(edit))
}

async fn read(State(platform): State<Platform<Note>>, Viewer(_): Viewer) -> Response {
    axum::Json(platform.read(Note::clone)).into_response()
}

async fn edit(State(platform): State<Platform<Note>>, write: Write) -> Response {
    platform.write(&write, |note, claims| {
        let asked: Edit = write.json("An edit is { \"text\": \"...\", \"revision\": n }")?;
        note.edit(claims, asked)?;
        Ok(Reply::ok(&*note))
    })
}
