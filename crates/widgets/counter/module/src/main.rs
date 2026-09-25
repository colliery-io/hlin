//! The counter's module: the number, and a button either side of it.
//!
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s; what is here is
//! what the counter draws and asks for.

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The counter, as the widget describes it.
#[derive(Debug, Clone, Deserialize)]
struct Counter {
    value: u64,
    last: Option<Bump>,
}

#[derive(Debug, Clone, Deserialize)]
struct Bump {
    name: String,
    by: i64,
}

fn main() {
    start("counter", |widget: Widget| {
        let counter = widget.load::<Counter>(|| Request::get("/api/counter"));
        let read_only = widget.read_only();
        let bump = move |by: i64| {
            widget.send(
                Request::post("/api/counter/bump")
                    .json(&serde_json::json!({ "by": by }))
                    .expect("a number serialises"),
            )
        };

        loaded_view(counter, move |counter| {
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
        })
    });
}
