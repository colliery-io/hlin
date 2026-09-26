//! The quote's components: the quote, who said it, and two things to do with it.
//!
//! The quote moves on by itself on the hour, which nobody writes and so
//! nothing announces; this asks again every five minutes while it is in
//! view, which is soon enough for something that changes hourly.

use std::time::Duration;

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The widget's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

#[derive(Debug, Clone, Deserialize)]
struct Shown {
    id: usize,
    text: String,
    by: String,
    pinned: bool,
}

/// The whole widget.
#[component]
pub fn Quote() -> impl IntoView {
    let widget = use_widget();
    let visible = widget.visible();
    let tick = RwSignal::new(0_u32);
    set_interval(
        move || {
            if visible.get_untracked() {
                tick.update(|tick| *tick += 1);
            }
        },
        Duration::from_secs(300),
    );
    let shown = widget.load::<Shown>(move || {
        // Read, so the fetch is made again on every tick.
        tick.track();
        Request::get("/api/quote")
    });
    let read_only = widget.read_only();

    let drawn = loaded_view(shown, move |shown| {
        let pinned = shown.pinned;
        let pin = move |_| {
            widget.send(if pinned {
                Request::delete("/api/quote/pin")
            } else {
                Request::put("/api/quote/pin")
            })
        };
        view! {
            <figure class="quote" data-quote=shown.id>
                <blockquote class="quote__text">{shown.text}</blockquote>
                <figcaption class="quote__by">{format!("— {}", shown.by)}</figcaption>
            </figure>
            <div class="w-row">
                <button
                    class="w-button w-button--quiet"
                    aria-pressed=pinned.to_string()
                    disabled=move || read_only.get()
                    on:click=pin
                >
                    {if pinned { "Unpin" } else { "Pin" }}
                </button>
                <button
                    class="w-button w-button--quiet"
                    disabled=move || read_only.get() || pinned
                    on:click=move |_| widget.send(Request::post("/api/quote/next"))
                >
                    "Another"
                </button>
                <p class="w-quiet quote__note">
                    {if pinned { "Pinned: it stays until you unpin it." } else { "A new one every hour." }}
                </p>
            </div>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
