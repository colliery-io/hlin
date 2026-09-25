//! The shoutbox's module: the conversation, newest at the bottom, and a line
//! to say something on.
//!
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s; what is here is
//! what the shoutbox draws and asks for.

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The room, as the widget describes it to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Room {
    shouts: Vec<Shout>,
}

#[derive(Debug, Clone, Deserialize)]
struct Shout {
    id: u64,
    name: String,
    text: String,
    at_ms: f64,
    mine: bool,
}

/// `HH:MM` on the browser's own clock.
fn time_of(at_ms: f64) -> String {
    let date = js_sys::Date::new_0();
    date.set_time(at_ms);
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

fn main() {
    start("shoutbox", |widget: Widget| {
        let room = widget.load::<Room>(|| Request::get("/api/shouts"));
        let read_only = widget.read_only();
        let draft = RwSignal::new(String::new());

        let say = move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            let text = draft.get_untracked();
            if text.trim().is_empty() {
                return;
            }
            widget.send_then(
                Request::post("/api/shouts")
                    .json(&serde_json::json!({ "text": text }))
                    .expect("a string serialises"),
                move |happened| {
                    // Cleared only if it still says what was sent: anything
                    // typed while the shout was on its way is kept.
                    if happened && draft.get_untracked() == text {
                        draft.set(String::new());
                    }
                },
            );
        };

        let conversation = loaded_view(room, move |room| {
            if room.shouts.is_empty() {
                return view! { <p class="w-quiet shoutbox__empty">"Nobody has said anything yet."</p> }
                    .into_any();
            }
            // Newest first in the document, drawn bottom up, so the latest is
            // in view without scrolling (see module.css).
            room.shouts
                .into_iter()
                .rev()
                .map(|shout| {
                    let path = format!("/api/shouts/{}", shout.id);
                    let take_back = shout.mine.then(|| {
                        view! {
                            <button
                                class="shoutbox__take-back"
                                aria-label="Take this back"
                                title="Take this back"
                                disabled=move || read_only.get()
                                on:click=move |_| widget.send(Request::delete(path.clone()))
                            >
                                "×"
                            </button>
                        }
                    });
                    view! {
                        <li class="shoutbox__shout" class:shoutbox__shout--mine=shout.mine data-shout=shout.id>
                            <span class="shoutbox__who">{shout.name}</span>
                            <span class="shoutbox__text">{shout.text}</span>
                            <time class="shoutbox__when">{time_of(shout.at_ms)}</time>
                            {take_back}
                        </li>
                    }
                })
                .collect_view()
                .into_any()
        });

        view! {
            <ul class="shoutbox__log" aria-live="polite">{conversation}</ul>
            <form class="w-row shoutbox__say" on:submit=say>
                <input
                    class="w-input shoutbox__input"
                    aria-label="Say something"
                    placeholder="Say something…"
                    maxlength="280"
                    prop:value=move || draft.get()
                    on:input=move |event| draft.set(event_target_value(&event))
                    disabled=move || read_only.get()
                />
                <button class="w-button" type="submit" disabled=move || read_only.get()>
                    "Send"
                </button>
            </form>
        }
    });
}
