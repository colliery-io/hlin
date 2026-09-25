//! The poll's module: the question, a bar per option, and a vote.
//!
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s; what is here is
//! what the poll draws and asks for.

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The poll, as the widget describes it to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Tally {
    question: String,
    options: Vec<Counted>,
    total: usize,
    mine: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Counted {
    id: String,
    label: String,
    votes: usize,
}

fn main() {
    start("poll", |widget: Widget| {
        let tally = widget.load::<Tally>(|| Request::get("/api/poll"));
        let read_only = widget.read_only();

        loaded_view(tally, move |tally| {
            let total = tally.total;
            let mine = tally.mine.clone();
            let rows = tally
                .options
                .into_iter()
                .map(|option| {
                    let chosen = mine.as_deref() == Some(option.id.as_str());
                    let share = (option.votes * 100).checked_div(total).unwrap_or(0);
                    let id = option.id.clone();
                    let vote = move |_| {
                        widget.send(
                            Request::post("/api/poll/vote")
                                .json(&serde_json::json!({ "option": id }))
                                .expect("a string serialises"),
                        )
                    };
                    view! {
                        <li class="poll__option" class:poll__option--mine=chosen data-option=option.id.clone()>
                            <button
                                class="w-button"
                                class:w-button--quiet=!chosen
                                disabled=move || read_only.get() || chosen
                                on:click=vote
                            >
                                {option.label.clone()}
                            </button>
                            <span class="poll__bar" style=format!("--share: {share}%")></span>
                            <span class="poll__votes">{option.votes}</span>
                        </li>
                    }
                })
                .collect_view();
            let take_back = mine.is_some().then(|| {
                view! {
                    <button
                        class="w-button w-button--quiet"
                        disabled=move || read_only.get()
                        on:click=move |_| widget.send(Request::delete("/api/poll/vote"))
                    >
                        "Take my vote back"
                    </button>
                }
            });
            let votes = if total == 1 { "vote" } else { "votes" };
            view! {
                <p class="poll__question">{tally.question}</p>
                <ul class="w-list">{rows}</ul>
                <div class="w-row">
                    <p class="w-quiet poll__total">{format!("{total} {votes}")}</p>
                    {take_back}
                </div>
            }
        })
    });
}
