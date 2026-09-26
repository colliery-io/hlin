//! The on-call rota's components: who has the pager this week, large, with a
//! way to look back and ahead a week at a time, who is next, and the roster
//! with the current person lit.
//!
//! Mounted twice, unchanged: at the root of the rota's own origin by
//! `hlin-widget-oncall-ui`, and in a Hlin panel by
//! `hlin-widget-oncall-module`. Which week is shown is the components' own
//! business, kept here and asked of the widget by name; the rota itself is
//! worked out on the server. Everything shared with the other widgets
//! (fetching again when told, refusals) is `hlin-widget-ui`'s.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The rota's own look, against the `--hlin-*` tokens and the shared classes
/// in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// A week of the rota, as the widget describes it to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Week {
    turn: Turn,
    current: bool,
    next: Vec<Turn>,
    previous_week: String,
    following_week: String,
    roster: Vec<String>,
    yours: Option<Turn>,
}

#[derive(Debug, Clone, Deserialize)]
struct Turn {
    week: String,
    starts: String,
    ends: String,
    name: String,
}

/// `2026-09-21` as `21 Sep`, without a calendar library.
fn short(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split('-').skip(1);
    let month = parts.next().and_then(|m| m.parse::<usize>().ok());
    let day = parts.next().and_then(|d| d.parse::<u32>().ok());
    match (month, day) {
        (Some(month @ 1..=12), Some(day)) => format!("{day} {}", MONTHS[month - 1]),
        _ => date.to_string(),
    }
}

/// The whole widget.
#[component]
pub fn Oncall() -> impl IntoView {
    let widget = use_widget();
    // `None` is "this week", whatever week that is when it is asked.
    let showing = RwSignal::new(None::<String>);
    let week = widget.load::<Week>(move || match showing.get() {
        Some(week) => Request::get(format!("/api/oncall/{week}")),
        None => Request::get("/api/oncall"),
    });

    let drawn = loaded_view(week, move |week| {
        let (back, ahead) = (week.previous_week.clone(), week.following_week.clone());
        let on_call = week.turn.name.clone();
        let next = week
            .next
            .iter()
            .map(|turn| {
                view! {
                    <li class="oncall__next">
                        <span class="w-quiet">{short(&turn.starts)}</span>
                        <span>{turn.name.clone()}</span>
                    </li>
                }
            })
            .collect_view();
        let roster = week
            .roster
            .iter()
            .map(|name| {
                let now = *name == on_call;
                view! { <li class="oncall__person" class:oncall__person--on=now>{name.clone()}</li> }
            })
            .collect_view();
        let yours = week.yours.map(|turn| {
            view! { <p class="w-quiet">{format!("Your next turn: {} ({}).", turn.week, short(&turn.starts))}</p> }
        });
        let heading = if week.current {
            "On call this week".to_string()
        } else {
            format!("On call in {}", week.turn.week)
        };
        view! {
            <div class="oncall__head">
                <button class="w-button w-button--quiet" aria-label="The week before" on:click=move |_| showing.set(Some(back.clone()))>
                    "‹"
                </button>
                <div class="oncall__who" data-week=week.turn.week.clone()>
                    <span class="w-quiet">{heading}</span>
                    <span class="oncall__name">{on_call.clone()}</span>
                    <span class="w-quiet">{format!("{} to {}", short(&week.turn.starts), short(&week.turn.ends))}</span>
                </div>
                <button class="w-button w-button--quiet" aria-label="The week after" on:click=move |_| showing.set(Some(ahead.clone()))>
                    "›"
                </button>
            </div>
            <div class="oncall__below">
                <ul class="w-list oncall__queue" aria-label="Next">{next}</ul>
                <ul class="oncall__roster" aria-label="Roster">{roster}</ul>
            </div>
            {yours}
            <Show when=move || !week.current>
                <button class="w-button w-button--quiet oncall__today" on:click=move |_| showing.set(None)>
                    "Back to this week"
                </button>
            </Show>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_shortened_to_day_and_month() {
        assert_eq!(short("2026-09-21"), "21 Sep");
        assert_eq!(short("2027-01-04"), "4 Jan");
        assert_eq!(short("soon"), "soon");
    }
}
