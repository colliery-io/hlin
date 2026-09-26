//! The clock's components: your cities, ticking, and a way to change them.
//!
//! Mounted twice, unchanged: at the root of the clock's own origin by
//! `hlin-widget-clock-ui`, and in a Hlin panel by `hlin-widget-clock-module`.
//!
//! The widget sends each city's UTC offset as it is now; the components add
//! it to the browser's own clock once a second. No zone database in the
//! browser, which keeps the clock the size of the others.

use std::time::Duration;

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The clock's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

#[derive(Debug, Clone, Deserialize)]
struct Board {
    cities: Vec<Clock>,
    others: Vec<Other>,
}

#[derive(Debug, Clone, Deserialize)]
struct Clock {
    id: String,
    name: String,
    offset_minutes: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct Other {
    id: String,
    name: String,
}

/// `HH:MM:SS` at `offset_minutes` from UTC, given UTC in milliseconds.
fn time_at(utc_millis: i64, offset_minutes: i64) -> String {
    let seconds = (utc_millis / 1000 + offset_minutes * 60).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// The whole widget.
#[component]
pub fn Clocks() -> impl IntoView {
    let widget = use_widget();
    let board = widget.load::<Board>(|| Request::get("/api/clocks"));
    let read_only = widget.read_only();
    let now = RwSignal::new(js_sys::Date::now() as i64);
    set_interval(
        move || now.set(js_sys::Date::now() as i64),
        Duration::from_secs(1),
    );
    let picked = RwSignal::new(String::new());

    let drawn = loaded_view(board, move |board| {
        let removable = board.cities.len() > 1;
        let clocks = board
            .cities
            .into_iter()
            .map(|clock| {
                let offset = clock.offset_minutes;
                let path = format!("/api/clocks/{}", clock.id);
                view! {
                    <li class="clock" data-city=clock.id.clone()>
                        <span class="clock__name">{clock.name.clone()}</span>
                        <span class="clock__time">{move || time_at(now.get(), offset)}</span>
                        <button
                            class="w-button w-button--quiet clock__remove"
                            aria-label=format!("Remove {}", clock.name)
                            disabled=move || read_only.get() || !removable
                            on:click=move |_| widget.send(Request::delete(path.clone()))
                        >
                            "×"
                        </button>
                    </li>
                }
            })
            .collect_view();
        let choices = board
            .others
            .into_iter()
            .map(|other| view! { <option value=other.id>{other.name}</option> })
            .collect_view();
        let add = move |_| {
            let city = picked.get_untracked();
            if city.is_empty() {
                return;
            }
            widget.send_then(
                Request::post("/api/clocks")
                    .json(&serde_json::json!({ "city": city }))
                    .expect("a string serialises"),
                move |happened| {
                    if happened {
                        picked.set(String::new());
                    }
                },
            );
        };
        view! {
            <ul class="w-list">{clocks}</ul>
            <div class="w-row">
                <select
                    class="w-input"
                    aria-label="City"
                    prop:value=move || picked.get()
                    on:change=move |event| picked.set(event_target_value(&event))
                >
                    <option value="">"Add a city…"</option>
                    {choices}
                </select>
                <button class="w-button" disabled=move || read_only.get() on:click=add>
                    "Add"
                </button>
            </div>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_utc_plus_the_offset_wrapped_to_one_day() {
        // 2026-01-15T23:30:05Z
        let at = 1_768_519_805_000;
        assert_eq!(time_at(at, 0), "23:30:05");
        assert_eq!(time_at(at, 540), "08:30:05");
        assert_eq!(time_at(at, -300), "18:30:05");
    }
}
