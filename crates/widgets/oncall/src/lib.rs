//! Widget 16 of twenty ([[HLIN-I-0012]]): who is on call this week.
//!
//! A fixed roster that takes turns a week at a time, handing over at the
//! start of each ISO week (Monday, 00:00 UTC). Nobody writes to it: the rota
//! is the roster and the calendar, so every week's answer can be worked out,
//! this one or any other ([`on_call`]).
//!
//! Not shared, and nothing to announce: with no writes there is never a
//! change to tell anybody of. The week moves on by itself, which the module
//! shows the next time it asks (a remount, or the shell's context
//! changing) and the fallback by asking again hourly.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/oncall` | This week: who, the next few, and the roster |
//! | `GET` | `/api/oncall/{week}` | Any week, as `2026-W39` |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the next few weeks as a `table`.
//!
//! The rules:
//!
//! - The roster ([`ROSTER`]) takes turns in order, one ISO week each,
//!   without a break at the new year, whether it has 52 weeks or 53.
//! - A week is named as ISO 8601 names it, `YYYY-Www`; nothing else is a
//!   week.
//! - Anyone signed in may look; if they are on the roster (by name) they are
//!   told when their next turn is.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Viewer, Widget, display_name};
use serde::Serialize;
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "oncall";

/// Who takes turns, in order.
pub const ROSTER: &[&str] = &["Amara", "Bjørn", "Chen", "Dalia", "Emeka", "Freya"];

/// The Monday the rota counts from: ISO week 2024-W01, which was Amara's.
pub fn epoch() -> NaiveDate {
    NaiveDate::from_isoywd_opt(2024, 1, Weekday::Mon).expect("a real week")
}

/// How many turns ahead the rota shows.
pub const AHEAD: usize = 4;

/// The rota. It holds nothing that changes; it is a type so the widget has
/// something to be, and so a test can run it with another roster.
#[derive(Debug, Clone)]
pub struct Rota {
    roster: Vec<String>,
}

/// One week of the rota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Turn {
    /// The week, as `2026-W39`.
    pub week: String,
    /// Its Monday.
    pub starts: NaiveDate,
    /// Its Sunday.
    pub ends: NaiveDate,
    /// Who is on call.
    pub name: String,
}

/// A week of the rota as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Week {
    /// The week asked about.
    pub turn: Turn,
    /// Whether it is this week.
    pub current: bool,
    /// The weeks after it.
    pub next: Vec<Turn>,
    /// The week before, for going back.
    pub previous_week: String,
    /// The week after.
    pub following_week: String,
    /// Everybody on the roster, in turn order.
    pub roster: Vec<String>,
    /// If the viewer is on the roster, their next turn from the week asked
    /// about (that week included).
    pub yours: Option<Turn>,
}

/// The Monday of the week that contains `day`.
pub fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// `2026-W39`, for the week starting `monday`.
pub fn week_name(monday: NaiveDate) -> String {
    let week = monday.iso_week();
    format!("{}-W{:02}", week.year(), week.week())
}

/// The Monday of the week named `name` (`2026-W39`), if it names one.
pub fn parse_week(name: &str) -> Option<NaiveDate> {
    let (year, week) = name.split_once("-W")?;
    if week.len() != 2 || year.len() != 4 {
        return None;
    }
    NaiveDate::from_isoywd_opt(year.parse().ok()?, week.parse().ok()?, Weekday::Mon)
}

/// Who, of `roster`, is on call in the week starting `monday`.
pub fn on_call(roster: &[String], monday: NaiveDate) -> &str {
    let weeks = (monday - epoch()).num_weeks();
    let len = roster.len() as i64;
    &roster[weeks.rem_euclid(len) as usize]
}

impl Rota {
    /// The rota with [`ROSTER`].
    pub fn seed() -> Self {
        Self::new(ROSTER)
    }

    /// A rota with this roster, which must not be empty.
    pub fn new(roster: &[&str]) -> Self {
        assert!(!roster.is_empty(), "a rota needs somebody on it");
        Self {
            roster: roster.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn turn(&self, monday: NaiveDate) -> Turn {
        Turn {
            week: week_name(monday),
            starts: monday,
            ends: monday + Duration::days(6),
            name: on_call(&self.roster, monday).to_string(),
        }
    }

    /// The week starting `monday`, as `claims` sees it on `today`.
    pub fn week(&self, claims: &Claims, monday: NaiveDate, today: NaiveDate) -> Week {
        let me = display_name(claims);
        let on_roster = self.roster.contains(&me);
        Week {
            turn: self.turn(monday),
            current: monday == self::monday(today),
            next: (1..=AHEAD as i64)
                .map(|ahead| self.turn(monday + Duration::weeks(ahead)))
                .collect(),
            previous_week: week_name(monday - Duration::weeks(1)),
            following_week: week_name(monday + Duration::weeks(1)),
            roster: self.roster.clone(),
            yours: on_roster.then(|| {
                (0..self.roster.len() as i64)
                    .map(|ahead| self.turn(monday + Duration::weeks(ahead)))
                    .find(|turn| turn.name == me)
                    .expect("everybody on the roster has a turn within one round")
            }),
        }
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Rota> {
    Widget {
        panel: PANEL,
        name: "On call",
        icon: "phone",
        title: "On call",
        description: "Who is on call this week, and who is next",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // Changes once a week, by the calendar; hourly notices a Monday
            // soon enough.
            refresh_ms: Some(3_600_000),
            data: |rota, claims| {
                let today = Utc::now().date_naive();
                Envelope::Records(as_table(&rota.week(claims, monday(today), today)))
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This week and the next few, for the shell to draw where the module is not.
fn as_table(week: &Week) -> Records {
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("week", "Week"),
            column("from", "From"),
            column("name", "On call"),
        ],
        rows: std::iter::once(&week.turn)
            .chain(&week.next)
            .map(|turn| {
                let row = json!({
                    "week": turn.week,
                    "from": turn.starts.format("%a %-d %b").to_string(),
                    "name": turn.name,
                });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: Some(Utc::now()),
        extra: Default::default(),
    }
}

/// This widget's own routes. Reads only.
pub fn api() -> Router<Platform<Rota>> {
    Router::new()
        .route("/api/oncall", get(this_week))
        .route("/api/oncall/{week}", get(any_week))
}

async fn this_week(State(platform): State<Platform<Rota>>, Viewer(claims): Viewer) -> Response {
    let today = Utc::now().date_naive();
    axum::Json(platform.read(|rota| rota.week(&claims, monday(today), today))).into_response()
}

async fn any_week(
    State(platform): State<Platform<Rota>>,
    Path(week): Path<String>,
    Viewer(claims): Viewer,
) -> Response {
    let Some(asked) = parse_week(&week) else {
        return Refusal::not_found("There is no such week; weeks are written like 2026-W39")
            .into_response();
    };
    let today = Utc::now().date_naive();
    axum::Json(platform.read(|rota| rota.week(&claims, asked, today))).into_response()
}
