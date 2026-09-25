//! Widget 10 of twenty ([[HLIN-I-0012]]): a service health light.
//!
//! Not shared. The services and their health are made up, and the same for
//! everybody: each five-minute window of each service is healthy, degraded
//! or down, decided by the window and the service alone, so the demo shows
//! the same history every time and a test can say what it will be. Which
//! service your light watches is yours; choosing is announced, as every
//! widget's writes are, so your other browser follows.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/status` | Your service: its light, since when, the last hour, the last day |
//! | `PUT` | `/api/status/service` | `{ "service": id }`: watch another |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the light as a `status`.
//!
//! The rules:
//!
//! - Only the services there are: [`SERVICES`].
//! - The light is the current window's health; "since" is when the run of
//!   that health began, looking back at most a day.
//! - Uptime is the share of the last day's windows that were healthy.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use chrono::{DateTime, Utc};
use hlin_widget_support::envelope::{Envelope, Health, Status};
use hlin_widget_support::{Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "status";

/// How long one window of health lasts, in milliseconds.
pub const WINDOW: i64 = 5 * 60_000;

/// Windows in a day.
const DAY: i64 = 24 * 60 * 60_000 / WINDOW;

/// Windows in the last-hour strip.
pub const STRIP: usize = 12;

/// Every service: id, name, and how often, in a thousand windows, it is
/// degraded and down.
pub const SERVICES: &[(&str, &str, u64, u64)] = &[
    ("api", "Public API", 60, 15),
    ("database", "Database", 30, 10),
    ("queue", "Job queue", 90, 30),
    ("search", "Search", 120, 40),
];

/// A light's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Light {
    /// Healthy.
    Ok,
    /// Working, but not properly.
    Degraded,
    /// Not working.
    Down,
}

/// The health of `service` (its index in [`SERVICES`]) in window `window`
/// (milliseconds since the epoch divided by [`WINDOW`]).
pub fn light(service: usize, window: i64) -> Light {
    let (_, _, degraded, down) = SERVICES[service];
    let roll = mix(window as u64 ^ ((service as u64 + 1) << 48)) % 1000;
    if roll < down {
        Light::Down
    } else if roll < down + degraded {
        Light::Degraded
    } else {
        Light::Ok
    }
}

fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The time now, in epoch milliseconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Who watches which service, by `sub`. Nobody is in here until they choose;
/// until then, the first.
#[derive(Debug, Clone, Default)]
pub struct Lights {
    watching: BTreeMap<String, usize>,
}

/// One person's light, at one instant.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reading {
    /// The service's id.
    pub service: &'static str,
    /// Its name.
    pub name: &'static str,
    /// Its light now.
    pub light: Light,
    /// When the light turned this colour, epoch milliseconds; a day ago at
    /// most.
    pub since_ms: i64,
    /// The last hour, a window at a time, oldest first, now last.
    pub strip: Vec<Light>,
    /// The share of the last day's windows that were healthy, in percent to
    /// one decimal place.
    pub uptime: f64,
    /// The services there are, `[id, name]`.
    pub services: Vec<(&'static str, &'static str)>,
}

/// What choosing a service asks for.
#[derive(Debug, Deserialize)]
pub struct Choose {
    /// Its id.
    pub service: String,
}

impl Lights {
    fn watched(&self, who: &str) -> usize {
        self.watching.get(who).copied().unwrap_or(0)
    }

    /// Watch another service.
    pub fn choose(&mut self, who: &str, id: &str) -> Result<(), Refusal> {
        let Some(service) = SERVICES.iter().position(|(known, ..)| *known == id) else {
            return Err(Refusal::not_found("There is no such service"));
        };
        self.watching.insert(who.to_string(), service);
        Ok(())
    }

    /// `who`'s light at `now`.
    pub fn reading(&self, who: &str, now: i64) -> Reading {
        let service = self.watched(who);
        let (id, name, ..) = SERVICES[service];
        let current = now.div_euclid(WINDOW);
        let lit = light(service, current);
        let run = (1..DAY)
            .take_while(|back| light(service, current - back) == lit)
            .count() as i64;
        let healthy = (0..DAY)
            .filter(|back| light(service, current - back) == Light::Ok)
            .count();
        Reading {
            service: id,
            name,
            light: lit,
            since_ms: (current - run) * WINDOW,
            strip: (0..STRIP as i64)
                .rev()
                .map(|back| light(service, current - back))
                .collect(),
            uptime: (healthy as f64 * 1000.0 / DAY as f64).round() / 10.0,
            services: SERVICES.iter().map(|(id, name, ..)| (*id, *name)).collect(),
        }
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Lights> {
    Widget {
        panel: PANEL,
        name: "Status",
        icon: "heart-pulse",
        title: "Service status",
        description: "One service's health, as a light",
        shared: false,
        fallback: Some(Fallback {
            kind: "status",
            envelope: "status.v1",
            // The light changes on the five minutes without anybody writing.
            refresh_ms: Some(30_000),
            data: |lights, claims| {
                let reading = lights.reading(&claims.sub, now());
                Envelope::Status(Status {
                    status: match reading.light {
                        Light::Ok => Health::Ok,
                        Light::Degraded => Health::Degraded,
                        Light::Down => Health::Down,
                    },
                    label: Some(reading.name.to_string()),
                    detail: Some(format!("Up {}% of the last day", reading.uptime)),
                    since: DateTime::<Utc>::from_timestamp_millis(reading.since_ms),
                    items: vec![],
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Lights>> {
    Router::new()
        .route("/api/status", get(read))
        .route("/api/status/service", put(choose))
}

async fn read(State(platform): State<Platform<Lights>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|lights| lights.reading(&claims.sub, now()))).into_response()
}

async fn choose(State(platform): State<Platform<Lights>>, write: Write) -> Response {
    platform.write(&write, |lights, claims| {
        let asked: Choose = write.json("Choosing is { \"service\": id }")?;
        lights.choose(&claims.sub, &asked.service)?;
        Ok(Reply::ok(lights.reading(&claims.sub, now())))
    })
}
