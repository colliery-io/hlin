//! Widget 11 of twenty ([[HLIN-I-0012]]): synthetic weather for a few cities.
//!
//! Nobody's real forecast: the weather is made up, from the city and the
//! hour, the same way every time. That makes it the same for everybody who
//! looks in the same hour, different from one hour to the next, and testable
//! without a network or a clock that moves.
//!
//! Not shared: what each person chooses is how they see it, their home city
//! (drawn large, with the next hours) and their units. Nobody else's panel
//! changes when they change theirs. Writes are still announced, as every
//! widget's are, so the same person's other browser agrees.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/weather` | Your home city in detail, the others in brief, in your units |
//! | `PUT` | `/api/weather/home` | `{ "city": id }` |
//! | `PUT` | `/api/weather/units` | `{ "units": "c" }` or `{ "units": "f" }` |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: every city's weather now, as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in sees the weather. Until they choose, their home is
//!   Oslo and their units Celsius.
//! - The weather is a function of the city and the hour ([`reading`]), so
//!   everybody sees the same weather in the same hour.
//! - Only cities this widget knows ([`CITIES`]), and only `c` or `f`.

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "weather";

/// How many hours ahead the home city's forecast runs.
pub const HOURS_AHEAD: usize = 6;

/// A city this widget makes weather for.
#[derive(Debug, Clone, Copy)]
pub struct City {
    /// Stable, and what a choice names.
    pub id: &'static str,
    /// What it is called.
    pub name: &'static str,
    /// Hours ahead of UTC, all year: the weather is synthetic, so is the zone.
    pub utc_offset: i64,
    /// The yearly mean, in °C.
    pub mean: f64,
    /// How far summer and winter swing either side of the mean, in °C;
    /// negative south of the equator, where the seasons are the other way.
    pub seasons: f64,
    /// How wet it is, from 0 (never rains) to 1.
    pub wet: f64,
}

/// Every city there is weather for, in the order they are listed.
pub const CITIES: &[City] = &[
    City {
        id: "oslo",
        name: "Oslo",
        utc_offset: 1,
        mean: 6.0,
        seasons: 11.0,
        wet: 0.45,
    },
    City {
        id: "cairo",
        name: "Cairo",
        utc_offset: 2,
        mean: 22.0,
        seasons: 7.0,
        wet: 0.03,
    },
    City {
        id: "lima",
        name: "Lima",
        utc_offset: -5,
        mean: 19.0,
        seasons: -3.5,
        wet: 0.1,
    },
    City {
        id: "seoul",
        name: "Seoul",
        utc_offset: 9,
        mean: 12.5,
        seasons: 14.0,
        wet: 0.35,
    },
    City {
        id: "toronto",
        name: "Toronto",
        utc_offset: -5,
        mean: 8.5,
        seasons: 13.0,
        wet: 0.35,
    },
    City {
        id: "nairobi",
        name: "Nairobi",
        utc_offset: 3,
        mean: 18.0,
        seasons: -1.5,
        wet: 0.3,
    },
];

/// Where everybody's home is until they choose.
pub const DEFAULT_HOME: &str = "oslo";

/// Celsius or Fahrenheit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    /// Celsius, the default.
    #[default]
    C,
    /// Fahrenheit.
    F,
}

impl Units {
    /// `celsius` in these units, to the nearest degree.
    pub fn degrees(self, celsius: f64) -> i64 {
        match self {
            Self::C => celsius.round() as i64,
            Self::F => (celsius * 9.0 / 5.0 + 32.0).round() as i64,
        }
    }
}

/// What the sky is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Sky {
    /// Clear.
    Sunny,
    /// Some cloud.
    Cloudy,
    /// Rain.
    Rain,
    /// Snow, which needs it cold.
    Snow,
    /// Fog, mornings only.
    Fog,
    /// Wind.
    Windy,
}

/// One person's choices. Nobody has any until they choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choice {
    home: &'static str,
    units: Units,
}

impl Default for Choice {
    fn default() -> Self {
        Self {
            home: DEFAULT_HOME,
            units: Units::C,
        }
    }
}

/// Everybody's choices, by `sub`. The weather itself is not state: it is a
/// function of the hour.
#[derive(Debug, Clone, Default)]
pub struct Weather {
    chosen: BTreeMap<String, Choice>,
}

/// The weather, as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outlook {
    /// Their units.
    pub units: Units,
    /// Their home city, in detail.
    pub home: Detail,
    /// Every other city, in brief.
    pub others: Vec<Brief>,
}

/// A city in detail: now, today's range, and the next hours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detail {
    /// The city's id.
    pub id: &'static str,
    /// Its name.
    pub name: &'static str,
    /// The temperature now, in the viewer's units.
    pub temperature: i64,
    /// The sky now.
    pub sky: Sky,
    /// Today's high and low there, in the viewer's units.
    pub high: i64,
    /// Today's low.
    pub low: i64,
    /// The next [`HOURS_AHEAD`] hours, in order.
    pub hours: Vec<Hour>,
}

/// One hour of forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hour {
    /// The hour, on the city's own clock, 0 to 23.
    pub local_hour: u32,
    /// The temperature, in the viewer's units.
    pub temperature: i64,
    /// The sky.
    pub sky: Sky,
}

/// A city in brief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brief {
    /// The city's id.
    pub id: &'static str,
    /// Its name.
    pub name: &'static str,
    /// The temperature now, in the viewer's units.
    pub temperature: i64,
    /// The sky now.
    pub sky: Sky,
}

/// What choosing a home asks for.
#[derive(Debug, Deserialize)]
pub struct Home {
    /// The city's id.
    pub city: String,
}

/// What choosing units asks for.
#[derive(Debug, Deserialize)]
pub struct Choose {
    /// `c` or `f`.
    pub units: Units,
}

/// The city called `id`, if there is weather for it.
pub fn city(id: &str) -> Option<&'static City> {
    CITIES.iter().find(|city| city.id == id)
}

/// A number from 0 to 1 that depends only on `seed`: SplitMix64's finaliser,
/// so the weather is the same on every machine and every build.
fn noise(seed: u64) -> f64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1_u64 << 53) as f64
}

fn seed(city: &City, hour: i64, salt: u64) -> u64 {
    let name = city
        .id
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    name ^ (hour as u64).wrapping_mul(0x2545_F491_4F6C_DD1D) ^ salt
}

/// The weather in `city` in the hour that contains `at`: a temperature in
/// °C and a sky.
///
/// The season follows the day of the year, the day follows the city's own
/// clock (coldest before dawn, warmest mid-afternoon), and the rest is noise
/// that depends only on the city and the hour. The sky changes every three
/// hours, not every hour, because weather does not flicker.
pub fn reading(city: &City, at: DateTime<Utc>) -> (f64, Sky) {
    use std::f64::consts::TAU;

    let hour = at.timestamp().div_euclid(3600);
    let local = at + Duration::hours(city.utc_offset);
    // Warmest in late July in the north, late January in the south.
    let season = (TAU * (f64::from(local.ordinal()) - 200.0) / 365.0).cos() * city.seasons;
    // Warmest at 15:00, coldest at 03:00.
    let day = (TAU * (f64::from(local.hour()) - 15.0) / 24.0).cos() * 4.0;
    let jitter = (noise(seed(city, hour, 1)) - 0.5) * 3.0;
    let celsius = city.mean + season + day + jitter;

    let roll = noise(seed(city, hour.div_euclid(3), 2));
    let sky = if roll < city.wet {
        if celsius <= 1.0 { Sky::Snow } else { Sky::Rain }
    } else if roll < city.wet + 0.08 && (5..=9).contains(&local.hour()) {
        Sky::Fog
    } else if roll < city.wet + 0.15 {
        Sky::Windy
    } else if roll < city.wet + 0.45 {
        Sky::Cloudy
    } else {
        Sky::Sunny
    };
    (celsius, sky)
}

impl Weather {
    fn choice(&self, claims: &Claims) -> Choice {
        self.chosen.get(&claims.sub).copied().unwrap_or_default()
    }

    /// The weather as `claims` sees it at `now`.
    pub fn outlook(&self, claims: &Claims, now: DateTime<Utc>) -> Outlook {
        let Choice { home, units } = self.choice(claims);
        let home = city(home).expect("a home is always a known city");
        let (celsius, sky) = reading(home, now);

        // Today, on the city's own clock: every hour from its midnight.
        let local = now + Duration::hours(home.utc_offset);
        let midnight = now - Duration::hours(i64::from(local.hour()));
        let today: Vec<f64> = (0..24)
            .map(|hour| reading(home, midnight + Duration::hours(hour)).0)
            .collect();
        let high = today.iter().copied().fold(f64::MIN, f64::max);
        let low = today.iter().copied().fold(f64::MAX, f64::min);

        Outlook {
            units,
            home: Detail {
                id: home.id,
                name: home.name,
                temperature: units.degrees(celsius),
                sky,
                high: units.degrees(high.max(celsius)),
                low: units.degrees(low.min(celsius)),
                hours: (1..=HOURS_AHEAD as i64)
                    .map(|ahead| {
                        let at = now + Duration::hours(ahead);
                        let (celsius, sky) = reading(home, at);
                        Hour {
                            local_hour: (at + Duration::hours(home.utc_offset)).hour(),
                            temperature: units.degrees(celsius),
                            sky,
                        }
                    })
                    .collect(),
            },
            others: CITIES
                .iter()
                .filter(|other| other.id != home.id)
                .map(|other| {
                    let (celsius, sky) = reading(other, now);
                    Brief {
                        id: other.id,
                        name: other.name,
                        temperature: units.degrees(celsius),
                        sky,
                    }
                })
                .collect(),
        }
    }

    /// Make `id` `claims`'s home city.
    pub fn set_home(&mut self, claims: &Claims, id: &str) -> Result<(), Refusal> {
        let Some(city) = city(id) else {
            return Err(Refusal::not_found("There is no weather for that city"));
        };
        let mut choice = self.choice(claims);
        choice.home = city.id;
        self.chosen.insert(claims.sub.clone(), choice);
        Ok(())
    }

    /// Show `claims` the weather in `units`.
    pub fn set_units(&mut self, claims: &Claims, units: Units) {
        let mut choice = self.choice(claims);
        choice.units = units;
        self.chosen.insert(claims.sub.clone(), choice);
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Weather> {
    Widget {
        panel: PANEL,
        name: "Weather",
        icon: "cloud",
        title: "Weather",
        description: "Made-up weather for a few cities, the same for everybody each hour",
        shared: false,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            // The weather moves on by the hour without anybody writing, so
            // the shell has to ask again; every ten minutes is plenty.
            refresh_ms: Some(600_000),
            data: |weather, claims| {
                Envelope::Records(as_table(&weather.outlook(claims, Utc::now())))
            },
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The sky in a word, for a table.
pub fn sky_word(sky: Sky) -> &'static str {
    match sky {
        Sky::Sunny => "Sunny",
        Sky::Cloudy => "Cloudy",
        Sky::Rain => "Rain",
        Sky::Snow => "Snow",
        Sky::Fog => "Fog",
        Sky::Windy => "Windy",
    }
}

/// Every city now, home first, for the shell to draw where the module is not.
fn as_table(outlook: &Outlook) -> Records {
    let column = |key: &str, label: &str, value_type, unit: Option<&str>| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: unit.map(str::to_string),
        extra: Default::default(),
    };
    let unit = match outlook.units {
        Units::C => "°C",
        Units::F => "°F",
    };
    let home = std::iter::once((
        outlook.home.name,
        outlook.home.temperature,
        outlook.home.sky,
    ));
    let others = outlook
        .others
        .iter()
        .map(|brief| (brief.name, brief.temperature, brief.sky));
    Records {
        columns: vec![
            column("city", "City", ColumnType::String, None),
            column("sky", "Sky", ColumnType::String, None),
            column("temperature", "Now", ColumnType::Number, Some(unit)),
        ],
        rows: home
            .chain(others)
            .map(|(name, temperature, sky)| {
                let row = json!({ "city": name, "sky": sky_word(sky), "temperature": temperature });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: Some(Utc::now()),
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Weather>> {
    Router::new()
        .route("/api/weather", get(read))
        .route("/api/weather/home", put(home))
        .route("/api/weather/units", put(units))
}

async fn read(State(platform): State<Platform<Weather>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|weather| weather.outlook(&claims, Utc::now()))).into_response()
}

async fn home(State(platform): State<Platform<Weather>>, write: Write) -> Response {
    platform.write(&write, |weather, claims| {
        let asked: Home = write.json("Choosing a home is { \"city\": id }")?;
        weather.set_home(claims, &asked.city)?;
        Ok(Reply::ok(weather.outlook(claims, Utc::now())))
    })
}

async fn units(State(platform): State<Platform<Weather>>, write: Write) -> Response {
    platform.write(&write, |weather, claims| {
        let asked: Choose = write.json("Units are { \"units\": \"c\" } or { \"units\": \"f\" }")?;
        weather.set_units(claims, asked.units);
        Ok(Reply::ok(weather.outlook(claims, Utc::now())))
    })
}
