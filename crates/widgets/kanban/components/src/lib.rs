//! The column's components: the cards, top first, each movable, and a way to add
//! one at the bottom.
//!
//! Whether a card may be taken off is the widget's rule; this only
//! offers the button on cards the widget says are yours, and shows the
//! widget's refusal if it says no anyway.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The widget's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

#[derive(Debug, Clone, Deserialize)]
struct Board {
    cards: Vec<Card>,
    most: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct Card {
    id: u64,
    text: String,
    by: String,
    mine: bool,
}

/// The whole widget.
#[component]
pub fn Kanban() -> impl IntoView {
    let widget = use_widget();
    let board = widget.load::<Board>(|| Request::get("/api/cards"));
    let read_only = widget.read_only();
    let draft = RwSignal::new(String::new());

    let shift = move |id: u64, by: i64| {
        widget.send(
            Request::post(format!("/api/cards/{id}/move"))
                .json(&serde_json::json!({ "by": by }))
                .expect("a number serialises"),
        )
    };
    let add = move || {
        let text = draft.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        widget.send_then(
            Request::post("/api/cards")
                .json(&serde_json::json!({ "text": text }))
                .expect("a string serialises"),
            move |happened| {
                if happened {
                    draft.set(String::new());
                }
            },
        );
    };

    let drawn = loaded_view(board, move |board| {
        let count = board.cards.len();
        let full = count >= board.most;
        let cards = board
            .cards
            .into_iter()
            .enumerate()
            .map(|(index, card)| {
                let id = card.id;
                let top = index == 0;
                let bottom = index + 1 == count;
                let done = card.mine.then(|| {
                    view! {
                        <button
                            class="w-button w-button--quiet kanban__done"
                            aria-label=format!("Take off “{}”", card.text)
                            disabled=move || read_only.get()
                            on:click=move |_| widget.send(Request::delete(format!("/api/cards/{id}")))
                        >
                            "✓"
                        </button>
                    }
                });
                view! {
                    <li class="kanban__card" data-card=id>
                        <div class="kanban__body">
                            <span class="kanban__text">{card.text.clone()}</span>
                            <span class="w-quiet">{card.by}</span>
                        </div>
                        <div class="kanban__moves">
                            <button
                                class="w-button w-button--quiet"
                                aria-label="Move up"
                                disabled=move || read_only.get() || top
                                on:click=move |_| shift(id, -1)
                            >
                                "↑"
                            </button>
                            <button
                                class="w-button w-button--quiet"
                                aria-label="Move down"
                                disabled=move || read_only.get() || bottom
                                on:click=move |_| shift(id, 1)
                            >
                                "↓"
                            </button>
                            {done}
                        </div>
                    </li>
                }
            })
            .collect_view();
        view! {
            // The column's title is the panel's, in the shell's header, and
            // the page's on its own UI; this does not say it twice.
            <div class="w-row kanban__head">
                <span class="w-quiet">{format!("{count} of {} cards", board.most)}</span>
            </div>
            <ol class="w-list kanban__cards">{cards}</ol>
            // Not a <form>: a sandboxed frame without `allow-forms` never
            // submits one, so Enter and the button each add directly.
            <div class="w-row kanban__add">
                <input
                    class="w-input"
                    aria-label="New card"
                    placeholder=if full { "The column is full" } else { "Add a card…" }
                    maxlength="80"
                    prop:value=move || draft.get()
                    on:input=move |event| draft.set(event_target_value(&event))
                    on:keydown=move |event| {
                        if event.key() == "Enter" {
                            add();
                        }
                    }
                />
                <button class="w-button" disabled=move || read_only.get() || full on:click=move |_| add()>
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
