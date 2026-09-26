//! The reactions' components: a pill per emoji with its count, yours lit, and
//! a click to react or take it back.
//!
//! Mounted twice, unchanged: at the root of the reactions' own origin by
//! `hlin-widget-reactions-ui`, and in a Hlin panel by
//! `hlin-widget-reactions-module`. Everything shared with the other widgets
//! (fetching again when told, writes and their refusals) is
//! `hlin-widget-ui`'s; what is here is what the reactions draw and ask for.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The reactions' own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// The reactions, as the widget describes them to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Board {
    reactions: Vec<Reaction>,
    left: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct Reaction {
    id: String,
    emoji: String,
    label: String,
    count: usize,
    names: Vec<String>,
    mine: bool,
}

/// Who reacted, for the pill's tooltip: "Alice, Bob and 3 more".
fn who(reaction: &Reaction) -> String {
    if reaction.count == 0 {
        return format!("{}: nobody yet", reaction.label);
    }
    let more = reaction.count.saturating_sub(reaction.names.len());
    let names = reaction.names.join(", ");
    if more == 0 {
        format!("{}: {names}", reaction.label)
    } else {
        format!("{}: {names} and {more} more", reaction.label)
    }
}

/// The whole widget.
#[component]
pub fn Reactions() -> impl IntoView {
    let widget = use_widget();
    let board = widget.load::<Board>(|| Request::get("/api/reactions"));
    let read_only = widget.read_only();

    let drawn = loaded_view(board, move |board| {
        let full = board.left == 0;
        let pills = board
            .reactions
            .into_iter()
            .map(|reaction| {
                let path = format!("/api/reactions/{}", reaction.id);
                let mine = reaction.mine;
                let toggle = move |_| {
                    widget.send(if mine {
                        Request::delete(path.clone())
                    } else {
                        Request::put(path.clone())
                    })
                };
                let title = who(&reaction);
                view! {
                    <button
                        class="reactions__pill"
                        class:reactions__pill--mine=mine
                        data-reaction=reaction.id.clone()
                        aria-pressed=if mine { "true" } else { "false" }
                        aria-label=title.clone()
                        title=title
                        disabled=move || read_only.get() || (full && !mine)
                        on:click=toggle
                    >
                        <span class="reactions__emoji">{reaction.emoji}</span>
                        <span class="reactions__count">{reaction.count}</span>
                    </button>
                }
            })
            .collect_view();
        let left = match board.left {
            0 => "You have used all your reactions; click one to take it back.".to_string(),
            1 => "One reaction left.".to_string(),
            n => format!("{n} reactions left."),
        };
        view! {
            <div class="reactions">{pills}</div>
            <p class="w-quiet">{left}</p>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
