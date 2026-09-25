//! Widget 17 of twenty ([[HLIN-I-0012]]): pick someone at random from the
//! team.
//!
//! The team is whoever has used the picker. It needs no directory and no
//! list to keep up to date: the shell's token says who is asking on every
//! request, so the picker remembers each person the first time it sees them,
//! by their stable id (`sub`), under the name the token gives (kept current).
//! Seeing somebody new is announced like a write, so everybody's picker
//! shows them.
//!
//! Shared: one team, one draw, and a pick is announced so every open picker
//! shows who it landed on.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/picker` | The people, who is in the draw, and the latest picks |
//! | `POST` | `/api/picker/pick` | Pick someone |
//! | `PUT` | `/api/picker/me` | `{ "in": false }` to sit out, `{ "in": true }` to come back |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the latest picks as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in who opens the picker is in the team, and in the draw
//!   until they sit out.
//! - Each person decides only whether they themselves are in the draw.
//! - A pick is at random from the people in the draw, and is never the
//!   person picked last time while there is anyone else to pick.
//! - Nobody in the draw, no pick.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use chrono::Utc;
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "picker";

/// How many picks the picker remembers.
pub const REMEMBERED: usize = 5;

/// The team, the draw, and the latest picks.
#[derive(Debug, Default)]
pub struct Picker {
    /// Everybody seen, by `sub`.
    ///
    /// Behind its own lock because a read adds to it: whoever looks is in
    /// the team, and a read has the state only to look at
    /// (`Platform::read`). The platform's lock is held around this one
    /// whenever it is taken, so it is never contended.
    people: Mutex<BTreeMap<String, Person>>,
    picks: VecDeque<Pick>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Person {
    name: String,
    in_draw: bool,
}

/// One pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pick {
    /// Who it landed on, by name.
    pub name: String,
    /// Their `sub`, so the next pick can pass them over.
    #[serde(skip)]
    pub sub: String,
    /// Who picked.
    pub by: String,
    /// When, in milliseconds since the epoch.
    pub at_ms: i64,
}

/// The picker as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Team {
    /// Everybody seen, by name.
    pub people: Vec<Member>,
    /// How many are in the draw.
    pub in_draw: usize,
    /// The latest picks, newest first.
    pub picks: Vec<Pick>,
}

/// One person, as the viewer sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Member {
    /// What they are called.
    pub name: String,
    /// Whether a pick could land on them.
    pub in_draw: bool,
    /// Whether they are the viewer.
    pub me: bool,
    /// Whether the latest pick landed on them.
    pub picked: bool,
}

/// What sitting out, or coming back, asks for.
#[derive(Debug, Deserialize)]
pub struct Draw {
    /// Whether to be in the draw.
    #[serde(rename = "in")]
    pub in_draw: bool,
}

impl Picker {
    /// Remember `claims`, or bring their name up to date. Whether they are
    /// new to the picker.
    pub fn saw(&self, claims: &Claims) -> bool {
        let name = display_name(claims);
        let mut people = self.people.lock().expect("not poisoned");
        match people.get_mut(&claims.sub) {
            Some(person) => {
                person.name = name;
                false
            }
            None => {
                people.insert(
                    claims.sub.clone(),
                    Person {
                        name,
                        in_draw: true,
                    },
                );
                true
            }
        }
    }

    /// The picker as `claims` sees it.
    pub fn team(&self, claims: &Claims) -> Team {
        let people = self.people.lock().expect("not poisoned");
        let last = self.picks.front().map(|pick| pick.sub.as_str());
        let mut members: Vec<Member> = people
            .iter()
            .map(|(sub, person)| Member {
                name: person.name.clone(),
                in_draw: person.in_draw,
                me: *sub == claims.sub,
                picked: Some(sub.as_str()) == last,
            })
            .collect();
        members.sort_by_key(|member| member.name.to_lowercase());
        Team {
            in_draw: people.values().filter(|person| person.in_draw).count(),
            people: members,
            picks: self.picks.iter().cloned().collect(),
        }
    }

    /// Put `claims` in the draw, or take them out of it.
    pub fn set_in_draw(&mut self, claims: &Claims, in_draw: bool) {
        self.saw(claims);
        let people = self.people.get_mut().expect("not poisoned");
        if let Some(person) = people.get_mut(&claims.sub) {
            person.in_draw = in_draw;
        }
    }

    /// Pick someone as `claims`, at `at_ms`, with `roll` as the dice: the
    /// candidates are counted off in id order and `roll` chooses among them.
    pub fn pick(&mut self, claims: &Claims, roll: u64, at_ms: i64) -> Result<&Pick, Refusal> {
        self.saw(claims);
        let last = self.picks.front().map(|pick| pick.sub.clone());
        let people = self.people.get_mut().expect("not poisoned");
        let in_draw: Vec<(&String, &Person)> =
            people.iter().filter(|(_, person)| person.in_draw).collect();
        if in_draw.is_empty() {
            return Err(Refusal::conflict(
                "Nobody is in the draw; somebody has to be in it to be picked",
            ));
        }
        // Not the same person twice running, while there is anybody else.
        let fresh: Vec<_> = in_draw
            .iter()
            .filter(|(sub, _)| Some(sub.as_str()) != last.as_deref())
            .collect();
        let candidates: Vec<_> = if fresh.is_empty() {
            in_draw.iter().collect()
        } else {
            fresh.into_iter().collect()
        };
        let (sub, person) = candidates[(roll % candidates.len() as u64) as usize];
        self.picks.push_front(Pick {
            name: person.name.clone(),
            sub: (*sub).clone(),
            by: display_name(claims),
            at_ms,
        });
        self.picks.truncate(REMEMBERED);
        Ok(self.picks.front().expect("just picked"))
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Picker> {
    Widget {
        panel: PANEL,
        name: "Picker",
        icon: "shuffle",
        title: "Picker",
        description: "Pick someone at random from the people here",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |picker, claims| Envelope::Records(as_table(&picker.team(claims))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The latest picks, for the shell to draw where the module is not.
fn as_table(team: &Team) -> Records {
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![column("name", "Picked"), column("by", "By")],
        rows: team
            .picks
            .iter()
            .map(|pick| {
                let row = json!({ "name": pick.name, "by": pick.by });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Picker>> {
    Router::new()
        .route("/api/picker", get(read))
        .route("/api/picker/pick", post(pick))
        .route("/api/picker/me", put(me))
}

async fn read(State(platform): State<Platform<Picker>>, Viewer(claims): Viewer) -> Response {
    let (new, team) = platform.read(|picker| (picker.saw(&claims), picker.team(&claims)));
    if new {
        // Somebody new is news to everybody else's picker.
        platform.announce();
    }
    axum::Json(team).into_response()
}

async fn pick(State(platform): State<Platform<Picker>>, write: Write) -> Response {
    platform.write(&write, |picker, claims| {
        let roll = getrandom::u64().map_err(|_| {
            Refusal::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "The picker could not roll its dice; try again",
            )
        })?;
        picker.pick(claims, roll, Utc::now().timestamp_millis())?;
        Ok(Reply::ok(picker.team(claims)))
    })
}

async fn me(State(platform): State<Platform<Picker>>, write: Write) -> Response {
    platform.write(&write, |picker, claims| {
        let asked: Draw =
            write.json("Sitting out is { \"in\": false }; coming back, { \"in\": true }")?;
        picker.set_in_draw(claims, asked.in_draw);
        Ok(Reply::ok(picker.team(claims)))
    })
}
