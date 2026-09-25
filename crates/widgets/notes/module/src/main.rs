//! The note's module: a sticky note, and a way to rewrite it.
//!
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s; what is here is
//! the note, the draft, and what happens when somebody else saved first.
//!
//! The draft lives outside the fetched note, so another person's edit arriving
//! mid-sentence redraws the note without losing a word of the draft. If their
//! edit landed first, the widget refuses this one; the module then shows their
//! text beside the draft, and offers to save over it knowingly.

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The longest a note may be; the widget's own rule, repeated so the counter
/// can say so before the widget has to.
const LONGEST: usize = 500;

#[derive(Debug, Clone, Deserialize)]
struct Note {
    text: String,
    revision: u64,
    by: Option<String>,
}

fn main() {
    start("notes", |widget: Widget| {
        let note = widget.load::<Note>(|| Request::get("/api/note"));
        let read_only = widget.read_only();
        // The revision the draft was started from, while there is a draft.
        let editing = RwSignal::new(None::<u64>);
        let draft = RwSignal::new(String::new());

        let save = move |revision: u64| {
            widget.send_then(
                Request::put("/api/note")
                    .json(
                        &serde_json::json!({ "text": draft.get_untracked(), "revision": revision }),
                    )
                    .expect("a string serialises"),
                move |happened| {
                    if happened {
                        editing.set(None);
                    }
                },
            );
        };

        loaded_view(note, move |note| {
            let by = note.by.clone().map_or_else(
                || "Nobody has edited it yet.".to_string(),
                |by| format!("Last edited by {by}"),
            );
            let current = note.clone();
            let mode = move || match editing.get() {
                None => {
                    let empty = current.text.is_empty();
                    let text = if empty {
                        "(empty)".to_string()
                    } else {
                        current.text.clone()
                    };
                    let start_editing = {
                        let current = current.clone();
                        move |_| {
                            draft.set(current.text.clone());
                            editing.set(Some(current.revision));
                        }
                    };
                    view! {
                        <p class="note__text" class:note__text--empty=empty>{text}</p>
                        <div class="w-row">
                            <p class="w-quiet note__by">{by.clone()}</p>
                            <button
                                class="w-button w-button--quiet"
                                disabled=move || read_only.get()
                                on:click=start_editing
                            >
                                "Edit"
                            </button>
                        </div>
                    }
                    .into_any()
                }
                Some(started) => {
                    // Somebody saved while this draft was being written: show
                    // what the note says now, and save over it only if asked.
                    let overtaken = (current.revision != started).then(|| {
                        let theirs = current.text.clone();
                        let who = current.by.clone().unwrap_or_else(|| "Someone".to_string());
                        let latest = current.revision;
                        view! {
                            <div class="note__theirs">
                                <p class="w-quiet">{format!("{who} changed it to:")}</p>
                                <p class="note__text">{theirs}</p>
                                <button
                                    class="w-button w-button--quiet"
                                    disabled=move || read_only.get()
                                    on:click=move |_| save(latest)
                                >
                                    "Save mine over it"
                                </button>
                            </div>
                        }
                    });
                    let stale = current.revision != started;
                    view! {
                        {overtaken}
                        <textarea
                            class="w-input note__draft"
                            aria-label="Note"
                            maxlength=LONGEST.to_string()
                            prop:value=draft.get_untracked()
                            on:input=move |event| draft.set(event_target_value(&event))
                        ></textarea>
                        <div class="w-row">
                            <p class="w-quiet note__count">
                                {move || format!("{} / {LONGEST}", draft.with(|text| text.chars().count()))}
                            </p>
                            <button
                                class="w-button w-button--quiet"
                                on:click=move |_| editing.set(None)
                            >
                                "Cancel"
                            </button>
                            <button
                                class="w-button"
                                disabled=move || read_only.get() || stale
                                on:click=move |_| save(started)
                            >
                                "Save"
                            </button>
                        </div>
                    }
                    .into_any()
                }
            };
            view! { <div class="note" data-revision=note.revision>{mode}</div> }
        })
    });
}
