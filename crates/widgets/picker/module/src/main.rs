//! The picker's module: whoever was picked last, large, a button to pick
//! again, and everybody the picker has seen, with a way to sit yourself out.
//!
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s; what is here is
//! what the picker draws and asks for. The dice are the server's.

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The picker, as the widget describes it to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Team {
    people: Vec<Member>,
    in_draw: usize,
    picks: Vec<Pick>,
}

#[derive(Debug, Clone, Deserialize)]
struct Member {
    name: String,
    in_draw: bool,
    me: bool,
    picked: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Pick {
    name: String,
    by: String,
    at_ms: i64,
}

fn main() {
    start("picker", |widget: Widget| {
        let team = widget.load::<Team>(|| Request::get("/api/picker"));
        let read_only = widget.read_only();

        loaded_view(team, move |team| {
            let latest = team.picks.first().cloned();
            let earlier = team
                .picks
                .iter()
                .skip(1)
                .map(|pick| pick.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let am_in = team.people.iter().any(|member| member.me && member.in_draw);
            let people = team
                .people
                .into_iter()
                .map(|member| {
                    view! {
                        <li
                            class="picker__person"
                            class:picker__person--out=!member.in_draw
                            class:picker__person--picked=member.picked
                            class:picker__person--me=member.me
                        >
                            {member.name}
                        </li>
                    }
                })
                .collect_view();
            let nobody = team.in_draw == 0;
            let (picked, by, at) = match latest {
                Some(pick) => (
                    pick.name,
                    format!("picked by {}", pick.by),
                    pick.at_ms.to_string(),
                ),
                None => (
                    "Nobody yet".to_string(),
                    "Pick someone to start".to_string(),
                    String::new(),
                ),
            };
            view! {
                <div class="picker__stage" data-picked-at=at>
                    <span class="picker__name">{picked}</span>
                    <span class="w-quiet">{by}</span>
                </div>
                <div class="w-row">
                    <button
                        class="w-button picker__pick"
                        disabled=move || read_only.get() || nobody
                        on:click=move |_| widget.send(Request::post("/api/picker/pick"))
                    >
                        "Pick someone"
                    </button>
                    <button
                        class="w-button w-button--quiet"
                        disabled=move || read_only.get()
                        on:click=move |_| {
                            widget.send(
                                Request::put("/api/picker/me")
                                    .json(&serde_json::json!({ "in": !am_in }))
                                    .expect("a bool serialises"),
                            )
                        }
                    >
                        {if am_in { "Sit me out" } else { "Put me back in" }}
                    </button>
                    <span class="w-quiet">{format!("{} in the draw", team.in_draw)}</span>
                </div>
                <ul class="picker__people" aria-label="The team">{people}</ul>
                <Show when={
                    let empty = earlier.is_empty();
                    move || !empty
                }>
                    <p class="w-quiet">{format!("Before: {earlier}")}</p>
                </Show>
            }
        })
    });
}
