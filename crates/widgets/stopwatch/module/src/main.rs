//! The stopwatch's module: the time, ticking, and the buttons a stopwatch has.
//!
//! The platform says how much had elapsed when it answered; the module counts
//! on from the moment the answer arrived, with the browser's own clock, ten
//! times a second while the panel is in view. It never compares its clock
//! with the platform's, so a browser whose clock is wrong still counts right.

use std::time::Duration;

use hlin_module::Request;
use hlin_widget_module::{Loaded, Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Face {
    elapsed_ms: u64,
    running: bool,
    laps: Vec<Lap>,
}

#[derive(Debug, Clone, Deserialize)]
struct Lap {
    number: usize,
    at_ms: u64,
    split_ms: u64,
}

/// `1:02:03.4`, `2:03.4`.
fn reading(ms: u64) -> String {
    let tenths = ms / 100 % 10;
    let seconds = ms / 1000 % 60;
    let minutes = ms / 60_000 % 60;
    let hours = ms / 3_600_000;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{tenths}")
    } else {
        format!("{minutes}:{seconds:02}.{tenths}")
    }
}

fn main() {
    start("stopwatch", |widget: Widget| {
        let face = widget.load::<Face>(|| Request::get("/api/stopwatch"));
        let read_only = widget.read_only();
        let visible = widget.module().visible();

        // When the latest answer arrived, by this browser's clock.
        let arrived = RwSignal::new(js_sys::Date::now());
        Effect::new(move |_| {
            if matches!(face.get(), Loaded::Ready(_)) {
                arrived.set(js_sys::Date::now());
            }
        });
        let now = RwSignal::new(js_sys::Date::now());
        set_interval(
            move || {
                if visible.get_untracked() {
                    now.set(js_sys::Date::now());
                }
            },
            Duration::from_millis(100),
        );

        let press =
            move |what: &'static str| widget.send(Request::post(format!("/api/stopwatch/{what}")));

        loaded_view(face, move |face| {
            let running = face.running;
            let banked = face.elapsed_ms;
            let elapsed = move || {
                let since = if running {
                    (now.get() - arrived.get()).max(0.0) as u64
                } else {
                    0
                };
                reading(banked + since)
            };
            let laps = face
                .laps
                .iter()
                .rev()
                .map(|lap| {
                    view! {
                        <li class="stopwatch__lap">
                            <span class="w-quiet">{format!("Lap {}", lap.number)}</span>
                            <span>{reading(lap.split_ms)}</span>
                            <span class="w-quiet">{reading(lap.at_ms)}</span>
                        </li>
                    }
                })
                .collect_view();
            // Start or stop; then lap while running, reset while stopped.
            let (first, first_label) = if running {
                ("stop", "Stop")
            } else {
                ("start", "Start")
            };
            let (second, second_label) = if running {
                ("lap", "Lap")
            } else {
                ("reset", "Reset")
            };
            let nothing_to_reset = !running && banked == 0;
            view! {
                <div class="stopwatch" class:stopwatch--running=running data-running=running.to_string()>
                    <span class="w-big stopwatch__time">{elapsed}</span>
                    <div class="w-row">
                        <button class="w-button" disabled=move || read_only.get() on:click=move |_| press(first)>
                            {first_label}
                        </button>
                        <button
                            class="w-button w-button--quiet"
                            disabled=move || read_only.get() || nothing_to_reset
                            on:click=move |_| press(second)
                        >
                            {second_label}
                        </button>
                    </div>
                </div>
                <ul class="w-list stopwatch__laps">{laps}</ul>
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopwatch_reads_as_the_platform_writes_it() {
        assert_eq!(reading(0), "0:00.0");
        assert_eq!(reading(62_345), "1:02.3");
        assert_eq!(reading(3_723_400), "1:02:03.4");
    }
}
