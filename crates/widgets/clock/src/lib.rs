//! Widget 1 of twenty ([[HLIN-I-0012]]): world clocks.
//!
//! Not shared: each person keeps their own list of cities, and nobody else's
//! changes when they change it. Writes are still announced, as every widget's
//! are, so the same person's other browser agrees; the panel does not declare
//! `pushed`, because nothing one person does is news to anybody else.
//!
//! The module ticks by itself. It is sent each city's current UTC offset, and
//! adds it to the browser's clock, so the zone database stays on the server
//! (daylight saving included) and the module stays small.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/clocks` | Your cities, with their offsets, and the ones you could add |
//! | `POST` | `/api/clocks` | `{ "city": id }`: add a clock |
//! | `DELETE` | `/api/clocks/{city}` | Remove one |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: your cities and their times, as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in has clocks. Until they change them, London, New York
//!   and Tokyo.
//! - Only cities this widget knows ([`CITIES`]).
//! - At most six clocks, and at least one: an empty clock widget is a
//!   mistake, not a choice.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use chrono::{DateTime, Offset, Utc};
use chrono_tz::Tz;
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "clock";

/// The most clocks one person may have.
pub const MOST: usize = 6;

/// Every city this widget keeps a clock for: id, name, zone.
pub const CITIES: &[(&str, &str, Tz)] = &[
    ("london", "London", chrono_tz::Europe::London),
    ("new-york", "New York", chrono_tz::America::New_York),
    ("tokyo", "Tokyo", chrono_tz::Asia::Tokyo),
    ("sydney", "Sydney", chrono_tz::Australia::Sydney),
    (
        "san-francisco",
        "San Francisco",
        chrono_tz::America::Los_Angeles,
    ),
    ("sao-paulo", "São Paulo", chrono_tz::America::Sao_Paulo),
    ("berlin", "Berlin", chrono_tz::Europe::Berlin),
    ("lagos", "Lagos", chrono_tz::Africa::Lagos),
    ("mumbai", "Mumbai", chrono_tz::Asia::Kolkata),
    ("singapore", "Singapore", chrono_tz::Asia::Singapore),
    ("auckland", "Auckland", chrono_tz::Pacific::Auckland),
    ("reykjavik", "Reykjavík", chrono_tz::Atlantic::Reykjavik),
];

/// Everybody starts with these.
pub const DEFAULT: &[&str] = &["london", "new-york", "tokyo"];

/// Everybody's clocks, by `sub`. Nobody is in here until they change theirs.
#[derive(Debug, Clone, Default)]
pub struct Clocks {
    chosen: BTreeMap<String, Vec<&'static str>>,
}

/// One person's clocks, as they see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    /// Their clocks, in the order they added them.
    pub cities: Vec<Clock>,
    /// Cities they could add.
    pub others: Vec<Other>,
}

/// One clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Clock {
    /// The city's id.
    pub id: &'static str,
    /// Its name.
    pub name: &'static str,
    /// Its zone, as IANA names it.
    pub zone: String,
    /// Minutes ahead of UTC, now.
    pub offset_minutes: i32,
}

/// A city that could be added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Other {
    /// The city's id.
    pub id: &'static str,
    /// Its name.
    pub name: &'static str,
}

/// What adding a clock asks for.
#[derive(Debug, Deserialize)]
pub struct Add {
    /// The city's id.
    pub city: String,
}

fn city(id: &str) -> Option<&'static (&'static str, &'static str, Tz)> {
    CITIES.iter().find(|(known, _, _)| *known == id)
}

impl Clocks {
    fn mine(&self, claims: &Claims) -> Vec<&'static str> {
        self.chosen
            .get(&claims.sub)
            .cloned()
            .unwrap_or_else(|| DEFAULT.to_vec())
    }

    /// `claims`'s clocks as they stand at `now`.
    pub fn board(&self, claims: &Claims, now: DateTime<Utc>) -> Board {
        let mine = self.mine(claims);
        Board {
            cities: mine
                .iter()
                .filter_map(|id| city(id))
                .map(|(id, name, zone)| Clock {
                    id,
                    name,
                    zone: zone.name().to_string(),
                    offset_minutes: now.with_timezone(zone).offset().fix().local_minus_utc() / 60,
                })
                .collect(),
            others: CITIES
                .iter()
                .filter(|(id, _, _)| !mine.contains(id))
                .map(|(id, name, _)| Other { id, name })
                .collect(),
        }
    }

    /// Add a clock to `claims`'s list.
    pub fn add(&mut self, claims: &Claims, id: &str) -> Result<(), Refusal> {
        let Some((id, _, _)) = city(id) else {
            return Err(Refusal::not_found("There is no clock for that city"));
        };
        let mut mine = self.mine(claims);
        if mine.contains(id) {
            return Err(Refusal::conflict("That clock is already on your list"));
        }
        if mine.len() >= MOST {
            return Err(Refusal::conflict("Six clocks is the most"));
        }
        mine.push(id);
        self.chosen.insert(claims.sub.clone(), mine);
        Ok(())
    }

    /// Take a clock off `claims`'s list.
    pub fn remove(&mut self, claims: &Claims, id: &str) -> Result<(), Refusal> {
        let mut mine = self.mine(claims);
        let Some(at) = mine.iter().position(|known| *known == id) else {
            return Err(Refusal::not_found("That clock is not on your list"));
        };
        if mine.len() == 1 {
            return Err(Refusal::conflict("Keep at least one clock"));
        }
        mine.remove(at);
        self.chosen.insert(claims.sub.clone(), mine);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Clocks> {
    Widget {
        panel: PANEL,
        name: "Clocks",
        icon: "clock",
        title: "World clocks",
        description: "The time where your colleagues are",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // The times move on without anybody writing, so the shell has to
            // ask again; twice a minute keeps a minute hand honest.
            refresh_ms: Some(30_000),
            data: |clocks, claims| Envelope::Records(as_table(&clocks.board(claims, Utc::now()))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// A clock's time at `now`, as `14:05`, and its offset, as `+09:00`.
pub fn reading(clock: &Clock, now: DateTime<Utc>) -> (String, String) {
    let offset = clock.offset_minutes;
    let local = now + chrono::Duration::minutes(offset.into());
    let sign = if offset < 0 { '-' } else { '+' };
    (
        local.format("%H:%M").to_string(),
        format!("{sign}{:02}:{:02}", offset.abs() / 60, offset.abs() % 60),
    )
}

/// The clocks as a table, for the shell to draw where the module is not.
fn as_table(board: &Board) -> Records {
    let now = Utc::now();
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("city", "City"),
            column("time", "Time"),
            column("offset", "UTC"),
        ],
        rows: board
            .cities
            .iter()
            .map(|clock| {
                let (time, offset) = reading(clock, now);
                let row = json!({ "city": clock.name, "time": time, "offset": offset });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: Some(now),
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Clocks>> {
    Router::new()
        .route("/api/clocks", get(read).post(add))
        .route("/api/clocks/{city}", delete(remove))
}

async fn read(State(platform): State<Platform<Clocks>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|clocks| clocks.board(&claims, Utc::now()))).into_response()
}

async fn add(State(platform): State<Platform<Clocks>>, write: Write) -> Response {
    platform.write(&write, |clocks, claims| {
        let asked: Add = write.json("Adding a clock is { \"city\": id }")?;
        clocks.add(claims, &asked.city)?;
        Ok(Reply::created(clocks.board(claims, Utc::now())))
    })
}

async fn remove(
    State(platform): State<Platform<Clocks>>,
    Path(city): Path<String>,
    write: Write,
) -> Response {
    platform.write(&write, |clocks, claims| {
        clocks.remove(claims, &city)?;
        Ok(Reply::ok(clocks.board(claims, Utc::now())))
    })
}
