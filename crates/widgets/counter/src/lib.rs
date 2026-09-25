//! Widget 2 of twenty ([[HLIN-I-0012]]): a number anyone can bump.
//!
//! The smallest widget that writes, and so the one to copy
//! (`hlin-widget-support`, *Adding a widget*). Shared: one person's bump is
//! everyone's number, so every bump is announced and every open counter
//! fetches again.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/counter` | The number, and who moved it last |
//! | `POST` | `/api/counter/bump` | `{ "by": 1 }` or `{ "by": -1 }` |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the number as a `stat`.
//!
//! The rules, which are this widget's and nobody else's:
//!
//! - Anyone signed in may read and bump.
//! - A bump is by one, up or down. Nothing else.
//! - The number never goes below zero.

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hlin_widget_support::envelope::{Envelope, Scalar};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "counter";

/// The counter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counter {
    /// The number.
    pub value: u64,
    /// Who moved it last, and which way; nobody, at first.
    pub last: Option<Bump>,
}

/// One bump, as the counter remembers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bump {
    /// Who, as they are called.
    pub name: String,
    /// Which way.
    pub by: i64,
}

/// What a bump asks for.
#[derive(Debug, Deserialize)]
pub struct BumpRequest {
    /// `1` or `-1`.
    pub by: i64,
}

impl Counter {
    /// Bump it, if the rules allow.
    pub fn bump(&mut self, claims: &Claims, by: i64) -> Result<(), Refusal> {
        let value = match by {
            1 => self.value + 1,
            -1 => self
                .value
                .checked_sub(1)
                .ok_or_else(|| Refusal::conflict("The counter is already at zero"))?,
            _ => return Err(Refusal::bad_request("A bump is by 1 or -1")),
        };
        self.value = value;
        self.last = Some(Bump {
            name: display_name(claims),
            by,
        });
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Counter> {
    Widget {
        panel: PANEL,
        name: "Counter",
        icon: "hash",
        title: "Counter",
        description: "A number anyone can bump",
        shared: true,
        fallback: Some(Fallback {
            kind: "stat",
            envelope: "scalar.v1",
            refresh_ms: None,
            data: |counter, _| {
                Envelope::Scalar(Scalar {
                    value: counter.value.into(),
                    unit: None,
                    label: counter
                        .last
                        .as_ref()
                        .map(|last| format!("Last bumped by {}", last.name)),
                    previous: None,
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Counter>> {
    Router::new()
        .route("/api/counter", get(read))
        .route("/api/counter/bump", post(bump))
}

async fn read(State(platform): State<Platform<Counter>>, Viewer(_): Viewer) -> Response {
    axum::Json(platform.read(Counter::clone)).into_response()
}

async fn bump(State(platform): State<Platform<Counter>>, write: Write) -> Response {
    platform.write(&write, |counter, claims| {
        let asked: BumpRequest = write.json("A bump is { \"by\": 1 } or { \"by\": -1 }")?;
        counter.bump(claims, asked.by)?;
        Ok(Reply::ok(&*counter))
    })
}
