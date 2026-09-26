//! The dice's components: pick the dice, roll, and see what everyone rolled.
//!
//! The platform rolls; this only asks, then draws the table as the platform
//! keeps it. Which die and how many are this browser's own choice, kept here
//! and not on the platform: nobody else's tray changes when you pick a d20.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The widget's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

#[derive(Debug, Clone, Deserialize)]
struct Table {
    rolls: Vec<Roll>,
    dice: Vec<u32>,
    most: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Roll {
    number: u64,
    name: String,
    sides: u32,
    faces: Vec<u32>,
    total: u32,
}

/// `3d6`, `d20`.
fn notation(sides: u32, count: usize) -> String {
    if count == 1 {
        format!("d{sides}")
    } else {
        format!("{count}d{sides}")
    }
}

/// The whole widget.
#[component]
pub fn Dice() -> impl IntoView {
    let widget = use_widget();
    let table = widget.load::<Table>(|| Request::get("/api/dice"));
    let read_only = widget.read_only();
    let sides = RwSignal::new(20_u32);
    let count = RwSignal::new(1_u32);
    let rolling = RwSignal::new(false);

    let roll = move |_| {
        rolling.set(true);
        widget.send_then(
            Request::post("/api/dice/roll")
                .json(&serde_json::json!({ "sides": sides.get_untracked(), "count": count.get_untracked() }))
                .expect("numbers serialise"),
            move |_| rolling.set(false),
        );
    };

    let drawn = loaded_view(table, move |table| {
        let most = table.most;
        let choices = table
            .dice
            .iter()
            .map(|&die| {
                view! {
                    <button
                        class="w-button dice__die"
                        class:w-button--quiet=move || sides.get() != die
                        aria-pressed=move || (sides.get() == die).to_string()
                        on:click=move |_| sides.set(die)
                    >
                        {format!("d{die}")}
                    </button>
                }
            })
            .collect_view();

        let mut rolls = table.rolls.into_iter();
        let latest = rolls.next().map(|roll| {
            let faces = roll
                .faces
                .iter()
                .map(|face| view! { <span class="dice__face">{*face}</span> })
                .collect_view();
            view! {
                <div class="dice__latest" data-number=roll.number>
                    <div class="dice__faces">{faces}</div>
                    <span class="w-big dice__total">{roll.total}</span>
                    <p class="w-quiet">
                        {format!("{} rolled {}", roll.name, notation(roll.sides, roll.faces.len()))}
                    </p>
                </div>
            }
        });
        let empty = latest.is_none().then(|| {
            view! { <p class="w-quiet dice__latest" data-number="0">"Nobody has rolled yet."</p> }
        });
        let history = rolls
            .map(|roll| {
                view! {
                    <li class="dice__past">
                        <span>{roll.name}</span>
                        <span class="w-quiet">{notation(roll.sides, roll.faces.len())}</span>
                        <span class="dice__past-total">{roll.total}</span>
                    </li>
                }
            })
            .collect_view();

        view! {
            <div class="w-row">{choices}</div>
            <div class="w-row">
                <button
                    class="w-button w-button--quiet"
                    aria-label="Fewer dice"
                    disabled=move || { count.get() <= 1 }
                    on:click=move |_| count.update(|count| *count -= 1)
                >
                    "−"
                </button>
                <span class="dice__count">{move || notation(sides.get(), count.get() as usize)}</span>
                <button
                    class="w-button w-button--quiet"
                    aria-label="More dice"
                    disabled=move || { count.get() >= most }
                    on:click=move |_| count.update(|count| *count += 1)
                >
                    "+"
                </button>
                <button
                    class="w-button dice__roll"
                    disabled=move || read_only.get() || rolling.get()
                    on:click=roll
                >
                    "Roll"
                </button>
            </div>
            {latest}
            {empty}
            <ul class="w-list dice__history">{history}</ul>
        }
    });
    view! {
        <style>{STYLE}</style>
        {drawn}
    }
}
