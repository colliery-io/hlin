//! The weather, called the way the shell calls it, and its rules at hours
//! chosen by the test rather than the clock.

use chrono::{DateTime, TimeZone, Utc};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{Claims, Widget};
use hlin_widget_weather::{
    CITIES, HOURS_AHEAD, PANEL, Sky, Units, Weather, api, city, reading, widget,
};
use serde_json::json;

async fn weather() -> Running {
    testing::start(widget(), Weather::default(), api()).await
}

fn claims(sub: &str) -> Claims {
    Claims {
        iss: "hlin".to_string(),
        sub: sub.to_string(),
        aud: PANEL.to_string(),
        iat: 0,
        exp: 0,
        jti: "t".to_string(),
        name: None,
        email: None,
        groups: Vec::new(),
        extra: Default::default(),
    }
}

fn at(month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, month, day, hour, minute, 0)
        .unwrap()
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    let described: Widget<Weather> = widget();
    assert_eq!(described.defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_table_that_is_asked_for_again() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(!panel.pushed, "one person's units are nobody else's news");
}

#[test]
fn the_weather_is_the_same_all_hour_and_for_everybody() {
    let oslo = city("oslo").unwrap();
    assert_eq!(
        reading(oslo, at(3, 4, 10, 1)),
        reading(oslo, at(3, 4, 10, 59))
    );

    let weather = Weather::default();
    let now = at(3, 4, 10, 30);
    assert_eq!(
        weather.outlook(&claims("u-alice"), now),
        weather.outlook(&claims("u-bob"), now)
    );
}

#[test]
fn the_weather_moves_on_and_differs_between_cities() {
    let oslo = city("oslo").unwrap();
    let cairo = city("cairo").unwrap();
    let day: Vec<f64> = (0..24)
        .map(|hour| reading(oslo, at(3, 4, hour, 0)).0)
        .collect();
    assert!(day.windows(2).any(|pair| pair[0] != pair[1]));
    assert_ne!(
        reading(oslo, at(3, 4, 10, 0)),
        reading(cairo, at(3, 4, 10, 0))
    );
}

#[test]
fn the_seasons_are_the_right_way_round_on_both_sides_of_the_equator() {
    let mean = |id: &str, month: u32| {
        let city = city(id).unwrap();
        (0..24)
            .map(|hour| reading(city, at(month, 15, hour, 0)).0)
            .sum::<f64>()
            / 24.0
    };
    assert!(mean("oslo", 7) > mean("oslo", 1) + 10.0);
    assert!(mean("lima", 1) > mean("lima", 7));
}

#[test]
fn snow_needs_it_cold_and_cairo_stays_dry() {
    for city in CITIES {
        for day in 1..=28 {
            for hour in 0..24 {
                let (celsius, sky) = reading(city, at(1, day, hour, 0));
                if sky == Sky::Snow {
                    assert!(celsius <= 1.0, "{} snowed at {celsius} °C", city.name);
                }
                assert!(
                    (-30.0..=50.0).contains(&celsius),
                    "{} at {celsius}",
                    city.name
                );
            }
        }
    }
    let cairo = city("cairo").unwrap();
    let wet = (0..24 * 28)
        .filter(|hour| {
            let (_, sky) = reading(cairo, at(1, 1, 0, 0) + chrono::Duration::hours(*hour));
            matches!(sky, Sky::Rain | Sky::Snow)
        })
        .count();
    assert!(wet < 24 * 28 / 10, "Cairo rained {wet} hours in four weeks");
}

#[test]
fn fahrenheit_is_celsius_converted_and_rounded() {
    assert_eq!(Units::C.degrees(21.6), 22);
    assert_eq!(Units::F.degrees(0.0), 32);
    assert_eq!(Units::F.degrees(-40.0), -40);
    assert_eq!(Units::F.degrees(21.6), 71);
}

#[test]
fn the_forecast_is_the_next_hours_on_the_citys_own_clock() {
    let weather = Weather::default();
    // 22:30 UTC is 23:30 in Oslo.
    let outlook = weather.outlook(&claims("u-alice"), at(3, 4, 22, 30));
    let hours: Vec<u32> = outlook
        .home
        .hours
        .iter()
        .map(|hour| hour.local_hour)
        .collect();
    assert_eq!(hours.len(), HOURS_AHEAD);
    assert_eq!(hours, vec![0, 1, 2, 3, 4, 5]);
    assert!(outlook.home.low <= outlook.home.temperature);
    assert!(outlook.home.temperature <= outlook.home.high);
    assert_eq!(outlook.others.len(), CITIES.len() - 1);
    assert!(outlook.others.iter().all(|brief| brief.id != "oslo"));
}

#[tokio::test]
async fn home_and_units_are_each_persons_own() {
    let weather = weather().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let homed = weather
        .write(
            &alice,
            "PUT",
            "/api/weather/home",
            Some(json!({ "city": "seoul" })),
        )
        .await;
    assert_eq!(homed.status, 200, "{}", homed.body);
    assert_eq!(homed.body["home"]["name"], "Seoul");
    let converted = weather
        .write(
            &alice,
            "PUT",
            "/api/weather/units",
            Some(json!({ "units": "f" })),
        )
        .await;
    assert_eq!(converted.body["units"], "f");

    let alices = weather.get(&alice, "/api/weather").await.body;
    assert_eq!(alices["home"]["id"], "seoul");
    assert_eq!(alices["units"], "f");
    let bobs = weather.get(&bob, "/api/weather").await.body;
    assert_eq!(bobs["home"]["id"], "oslo");
    assert_eq!(bobs["units"], "c");
}

#[tokio::test]
async fn only_a_known_city_or_unit_can_be_chosen() {
    let weather = weather().await;
    let alice = person("u-alice", "Alice");

    let nowhere = weather
        .write(
            &alice,
            "PUT",
            "/api/weather/home",
            Some(json!({ "city": "atlantis" })),
        )
        .await;
    assert_eq!(nowhere.status, 404);
    assert_eq!(
        nowhere.body,
        json!({ "message": "There is no weather for that city" })
    );

    let kelvin = weather
        .write(
            &alice,
            "PUT",
            "/api/weather/units",
            Some(json!({ "units": "k" })),
        )
        .await;
    assert_eq!(kelvin.status, 400);
}

#[tokio::test]
async fn a_choice_is_announced_on_the_event_stream() {
    let weather = weather().await;
    let alice = person("u-alice", "Alice");
    let mut events = weather.listen(&alice).await;

    weather
        .write(
            &alice,
            "PUT",
            "/api/weather/units",
            Some(json!({ "units": "f" })),
        )
        .await;

    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));
}

#[tokio::test]
async fn the_fallback_lists_every_city_home_first() {
    let weather = weather().await;
    let alice = person("u-alice", "Alice");
    weather
        .write(
            &alice,
            "PUT",
            "/api/weather/home",
            Some(json!({ "city": "lima" })),
        )
        .await;

    let Envelope::Records(table) = weather.fallback(&alice).await else {
        panic!("the weather's fallback is a table");
    };
    assert_eq!(table.rows.len(), CITIES.len());
    assert_eq!(table.rows[0]["city"], "Lima");
}
