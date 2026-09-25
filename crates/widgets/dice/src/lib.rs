//! Widget 5 of twenty ([[HLIN-I-0012]]): roll dice, with recent history.
//!
//! Shared: one table, one history. Everybody sees every roll, and who made
//! it, so a roll is announced and every open tray fetches again.
//!
//! The widget rolls, not the browser. A roll the browser made and reported
//! would be whatever the browser said it was; one the platform makes is one
//! nobody at the table can choose.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/dice` | The recent rolls, newest first, and the dice there are |
//! | `POST` | `/api/dice/roll` | `{ "sides": 20, "count": 1 }`: roll |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the recent rolls as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may read and roll.
//! - Only the dice a table has: [`DICE`].
//! - One to [`MOST_DICE`] of them at once.
//! - The table remembers the last [`KEPT`] rolls, newest first.

use std::collections::VecDeque;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "dice";

/// The dice on the table, by their number of sides.
pub const DICE: [u32; 6] = [4, 6, 8, 10, 12, 20];

/// The most dice rolled at once.
pub const MOST_DICE: u32 = 6;

/// How many rolls the table remembers.
pub const KEPT: usize = 10;

/// The table: its recent rolls, and where the next number comes from.
#[derive(Debug, Clone)]
pub struct Dice {
    rolls: VecDeque<Roll>,
    next: u64,
    rng: SplitMix,
}

/// One roll, as the table remembers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Roll {
    /// Rises by one per roll, so the module can tell a new roll that came out
    /// the same as the last from no roll at all.
    pub number: u64,
    /// Who rolled.
    pub name: String,
    /// Sides on each die.
    pub sides: u32,
    /// Each die, in the order it fell.
    pub faces: Vec<u32>,
    /// Their sum.
    pub total: u32,
}

/// What the table looks like to anyone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Table {
    /// Newest first.
    pub rolls: Vec<Roll>,
    /// The dice there are.
    pub dice: [u32; 6],
    /// The most at once.
    pub most: u32,
}

/// What a roll asks for.
#[derive(Debug, Deserialize)]
pub struct Throw {
    /// Sides on each die.
    pub sides: u32,
    /// How many dice; one if not said.
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

impl Default for Dice {
    /// A table whose dice nobody could have predicted.
    fn default() -> Self {
        Self::seeded(getrandom::u64().expect("the system has randomness"))
    }
}

impl Dice {
    /// A table whose dice fall the same way every time for the same `seed`,
    /// for tests.
    pub fn seeded(seed: u64) -> Self {
        Self {
            rolls: VecDeque::new(),
            next: 1,
            rng: SplitMix(seed),
        }
    }

    /// What anyone sees.
    pub fn table(&self) -> Table {
        Table {
            rolls: self.rolls.iter().cloned().collect(),
            dice: DICE,
            most: MOST_DICE,
        }
    }

    /// Roll, if the rules allow, and remember it.
    pub fn roll(&mut self, claims: &Claims, throw: &Throw) -> Result<Roll, Refusal> {
        if !DICE.contains(&throw.sides) {
            return Err(Refusal::bad_request(
                "There is no such die here: d4, d6, d8, d10, d12 or d20",
            ));
        }
        if !(1..=MOST_DICE).contains(&throw.count) {
            return Err(Refusal::bad_request(format!(
                "Roll one to {MOST_DICE} dice at once"
            )));
        }
        let faces: Vec<u32> = (0..throw.count)
            .map(|_| self.rng.below(throw.sides) + 1)
            .collect();
        let roll = Roll {
            number: self.next,
            name: display_name(claims),
            sides: throw.sides,
            total: faces.iter().sum(),
            faces,
        };
        self.next += 1;
        self.rolls.push_front(roll.clone());
        self.rolls.truncate(KEPT);
        Ok(roll)
    }
}

/// SplitMix64: small, fast, and good enough for dice among colleagues.
#[derive(Debug, Clone)]
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`: numbers from the uneven tail are thrown back, so no
    /// face comes up more often than another.
    fn below(&mut self, n: u32) -> u32 {
        let n = u64::from(n);
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let drawn = self.next();
            if drawn < zone {
                return (drawn % n) as u32;
            }
        }
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Dice> {
    Widget {
        panel: PANEL,
        name: "Dice",
        icon: "dice",
        title: "Dice",
        description: "Roll dice where everybody can see",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |dice, _| Envelope::Records(as_table(&dice.table())),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// `3d6`, `d20`: how a roll is written at a table.
pub fn notation(sides: u32, count: usize) -> String {
    if count == 1 {
        format!("d{sides}")
    } else {
        format!("{count}d{sides}")
    }
}

/// The recent rolls as a table, for the shell to draw where the module is not.
fn as_table(table: &Table) -> Records {
    let column = |key: &str, label: &str, value_type| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("who", "Who", ColumnType::String),
            column("roll", "Roll", ColumnType::String),
            column("total", "Total", ColumnType::Number),
        ],
        rows: table
            .rolls
            .iter()
            .map(|roll| {
                let faces: Vec<String> = roll.faces.iter().map(u32::to_string).collect();
                let row = json!({
                    "who": roll.name,
                    "roll": format!("{} ({})", notation(roll.sides, roll.faces.len()), faces.join(", ")),
                    "total": roll.total,
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Dice>> {
    Router::new()
        .route("/api/dice", get(read))
        .route("/api/dice/roll", post(roll))
}

async fn read(State(platform): State<Platform<Dice>>, Viewer(_): Viewer) -> Response {
    axum::Json(platform.read(Dice::table)).into_response()
}

async fn roll(State(platform): State<Platform<Dice>>, write: Write) -> Response {
    platform.write(&write, |dice, claims| {
        let asked: Throw = write.json("A roll is { \"sides\": 20, \"count\": 1 }")?;
        let roll = dice.roll(claims, &asked)?;
        Ok(Reply::created(roll))
    })
}
