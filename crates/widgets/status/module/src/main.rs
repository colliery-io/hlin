//! The light's module: one big light, since when, the last hour as a strip,
//! and which service it watches.
//!
//! The light changes on the five minutes, which nobody writes and so nothing
//! announces; the module asks again every thirty seconds while it is in view.

use std::time::Duration;

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Reading {
    service: String,
    light: String,
    since_ms: i64,
    strip: Vec<String>,
    uptime: f64,
    services: Vec<(String, String)>,
}

/// What a light says, in words.
fn word(light: &str) -> &'static str {
    match light {
        "ok" => "Healthy",
        "degraded" => "Degraded",
        "down" => "Down",
        _ => "Unknown",
    }
}

/// How long ago, as a person says it: `3 min`, `2 h 5 min`, `a day or more`.
fn ago(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "under a minute".to_string(),
        m if m < 60 => format!("{m} min"),
        m if m < 24 * 60 - 5 => format!("{} h {} min", m / 60, m % 60),
        _ => "a day or more".to_string(),
    }
}

fn main() {
    start("status", |widget: Widget| {
        let visible = widget.module().visible();
        let tick = RwSignal::new(0_u32);
        set_interval(
            move || {
                if visible.get_untracked() {
                    tick.update(|tick| *tick += 1);
                }
            },
            Duration::from_secs(30),
        );
        let reading = widget.load::<Reading>(move || {
            tick.track();
            Request::get("/api/status")
        });
        let read_only = widget.read_only();

        loaded_view(reading, move |reading| {
            let lit = reading.light.clone();
            let since = ago(js_sys::Date::now() as i64 - reading.since_ms);
            let strip = reading
                .strip
                .iter()
                .map(|light| {
                    view! { <span class=format!("status__cell status__cell--{light}") title=word(light)></span> }
                })
                .collect_view();
            let choices = reading
                .services
                .iter()
                .map(|(id, name)| {
                    view! { <option value=id.clone() selected={ *id == reading.service }>{name.clone()}</option> }
                })
                .collect_view();
            let choose = move |event| {
                widget.send(
                    Request::put("/api/status/service")
                        .json(&serde_json::json!({ "service": event_target_value(&event) }))
                        .expect("a string serialises"),
                )
            };
            view! {
                <div class="status" data-light=lit.clone()>
                    <span class=format!("status__light status__light--{lit}") aria-hidden="true"></span>
                    <div class="status__words">
                        <span class="status__word">{word(&lit)}</span>
                        <span class="w-quiet">{format!("for {since}")}</span>
                    </div>
                </div>
                <div class="status__strip" aria-label="The last hour">{strip}</div>
                <div class="w-row">
                    <select
                        class="w-input status__service"
                        aria-label="Service"
                        disabled=move || read_only.get()
                        on:change=choose
                    >
                        {choices}
                    </select>
                    <p class="w-quiet">{format!("Up {}% of the last day", reading.uptime)}</p>
                </div>
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_reads_as_a_person_says_it() {
        assert_eq!(ago(20_000), "under a minute");
        assert_eq!(ago(3 * 60_000), "3 min");
        assert_eq!(ago(125 * 60_000), "2 h 5 min");
        assert_eq!(ago(24 * 60 * 60_000), "a day or more");
    }
}
