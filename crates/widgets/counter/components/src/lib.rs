//! The counter's components: the number, and a button either side of it.
//!
//! Mounted twice, unchanged: at the root of the counter's own origin by
//! `hlin-widget-counter-ui`, and in a Hlin panel by
//! `hlin-widget-counter-module`. Which client they were given, neither they
//! nor this crate knows (`hlin-widget-ui`).

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The counter's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// The counter, as the widget describes it.
#[derive(Debug, Clone, Deserialize)]
struct Count {
    value: u64,
    last: Option<Bump>,
}

#[derive(Debug, Clone, Deserialize)]
struct Bump {
    name: String,
    by: i64,
}

/// The whole widget.
#[component]
pub fn Counter() -> impl IntoView {
    let widget = use_widget();
    let counter = widget.load::<Count>(|| Request::get("/api/counter"));
    let read_only = widget.read_only();
    let bump = move |by: i64| {
        widget.send(
            Request::post("/api/counter/bump")
                .json(&serde_json::json!({ "by": by }))
                .expect("a number serialises"),
        )
    };

    let drawn = loaded_view(counter, move |counter| {
        let last = counter.last.map(|last| {
            let way = if last.by > 0 { "up" } else { "down" };
            format!("{} bumped it {way}", last.name)
        });
        view! {
            <div class="counter">
                <button
                    class="w-button w-button--quiet"
                    aria-label="Bump down"
                    disabled=move || read_only.get() || counter.value == 0
                    on:click=move |_| bump(-1)
                >
                    "−"
                </button>
                <span class="w-big" data-value=counter.value>{counter.value}</span>
                <button
                    class="w-button"
                    aria-label="Bump up"
                    disabled=move || read_only.get()
                    on:click=move |_| bump(1)
                >
                    "+"
                </button>
            </div>
            <p class="w-quiet">{last.unwrap_or_else(|| "Nobody has bumped it yet.".to_string())}</p>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
