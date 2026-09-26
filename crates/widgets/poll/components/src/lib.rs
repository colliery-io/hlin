//! The poll's components: the question, a bar per option, and a vote.
//!
//! Mounted twice, unchanged: at the root of the poll's own origin by
//! `hlin-widget-poll-ui`, and in a Hlin panel by `hlin-widget-poll-module`.
//! Which vote is "mine" is the platform's to say, from whoever it decides is
//! asking: the shell's viewer under Hlin, its own sign-in on its own page.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The poll's own look, against the `--hlin-*` tokens and the shared classes
/// in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

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

/// The whole widget.
#[component]
pub fn Poll() -> impl IntoView {
    let widget = use_widget();
    let tally = widget.load::<Tally>(|| Request::get("/api/poll"));
    let read_only = widget.read_only();

    let drawn = loaded_view(tally, move |tally| {
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
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
