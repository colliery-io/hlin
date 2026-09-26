//! The weather's components: your home city large, the next hours under it, the
//! other cities as a row you can pick a new home from, and a unit switch.
//!
//! Everything shared with the other widgets (fetching again when told,
//! writes and their refusals) is `hlin-widget-ui`'s; what is here is
//! what the weather draws and asks for. The weather itself is made on the
//! server; this only draws it.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The widget's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

#[derive(Debug, Clone, Deserialize)]
struct Outlook {
    units: String,
    home: Detail,
    others: Vec<Brief>,
}

#[derive(Debug, Clone, Deserialize)]
struct Detail {
    name: String,
    temperature: i64,
    sky: String,
    high: i64,
    low: i64,
    hours: Vec<Hour>,
}

#[derive(Debug, Clone, Deserialize)]
struct Hour {
    local_hour: u32,
    temperature: i64,
    sky: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Brief {
    id: String,
    name: String,
    temperature: i64,
    sky: String,
}

/// A sky as one character, and as a word.
fn sky(sky: &str) -> (&'static str, &'static str) {
    match sky {
        "sunny" => ("☀", "Sunny"),
        "cloudy" => ("☁", "Cloudy"),
        "rain" => ("☂", "Rain"),
        "snow" => ("❄", "Snow"),
        "fog" => ("≡", "Fog"),
        "windy" => ("≋", "Windy"),
        _ => ("?", "Unknown"),
    }
}

/// The whole widget.
#[component]
pub fn Weather() -> impl IntoView {
    let widget = use_widget();
    let outlook = widget.load::<Outlook>(|| Request::get("/api/weather"));
    let read_only = widget.read_only();
    let choose = move |path: &'static str, body: serde_json::Value| {
        widget.send(Request::put(path).json(&body).expect("json serialises"))
    };

    let drawn = loaded_view(outlook, move |outlook| {
        let fahrenheit = outlook.units == "f";
        let degree = if fahrenheit { "°F" } else { "°C" };
        let home = outlook.home;
        let (glyph, word) = sky(&home.sky);
        let hours = home
            .hours
            .into_iter()
            .map(|hour| {
                let (glyph, word) = sky(&hour.sky);
                view! {
                    <li class="weather__hour" title=word>
                        <span class="w-quiet">{format!("{:02}", hour.local_hour)}</span>
                        <span class=format!("weather__glyph weather__glyph--{}", hour.sky)>{glyph}</span>
                        <span>{format!("{}°", hour.temperature)}</span>
                    </li>
                }
            })
            .collect_view();
        let others = outlook
            .others
            .into_iter()
            .map(|other| {
                let (glyph, word) = sky(&other.sky);
                let id = other.id.clone();
                view! {
                    <button
                        class="w-button w-button--quiet weather__other"
                        data-city=other.id.clone()
                        title=format!("{word}. Make {} your home", other.name)
                        disabled=move || read_only.get()
                        on:click=move |_| choose("/api/weather/home", serde_json::json!({ "city": id }))
                    >
                        <span class=format!("weather__glyph weather__glyph--{}", other.sky)>{glyph}</span>
                        {format!(" {} {}°", other.name, other.temperature)}
                    </button>
                }
            })
            .collect_view();
        let switch_to = if fahrenheit { "c" } else { "f" };
        view! {
            <div class="weather__now" data-sky=home.sky.clone()>
                <span class=format!("weather__glyph weather__glyph--big weather__glyph--{}", home.sky)>{glyph}</span>
                <div class="weather__reading">
                    <span class="w-big">{format!("{}{degree}", home.temperature)}</span>
                    <span class="w-quiet">{format!("{} · {word} · H {}° L {}°", home.name, home.high, home.low)}</span>
                </div>
                <button
                    class="w-button w-button--quiet weather__units"
                    aria-label="Switch units"
                    disabled=move || read_only.get()
                    on:click=move |_| choose("/api/weather/units", serde_json::json!({ "units": switch_to }))
                >
                    {if fahrenheit { "°C" } else { "°F" }}
                </button>
            </div>
            <ul class="weather__hours">{hours}</ul>
            <div class="w-row">{others}</div>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
