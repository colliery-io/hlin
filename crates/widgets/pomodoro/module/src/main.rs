//! The focus timer's module: a ring that empties as the phase runs, the time
//! left in the middle, and the buttons that make sense for where it is.
//!
//! The widget says how long is left when asked; the module counts down from
//! there with the browser's clock, once a second, so the server never ticks.
//! Everything shared with the other widgets (connecting, fetching again when
//! told, writes and their refusals) is `hlin-widget-module`'s.

use std::time::Duration;

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

/// The timer, as the widget describes it to its owner.
#[derive(Debug, Clone, Deserialize)]
struct Timer {
    phase: Option<String>,
    running: bool,
    paused: bool,
    left_ms: i64,
    length_ms: i64,
    finished: u32,
}

/// How long a focus is, shown on the face while nothing is running.
const FOCUS_MS: i64 = 25 * 60 * 1000;

/// `MM:SS`, rounding up, so a phase shows `25:00` as it starts and `00:00`
/// only once it is over.
fn clock(left_ms: i64) -> String {
    let seconds = (left_ms.max(0) + 999) / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn main() {
    start("pomodoro", |widget: Widget| {
        let timer = widget.load::<Timer>(|| Request::get("/api/pomodoro"));
        let read_only = widget.read_only();
        let now = RwSignal::new(js_sys::Date::now());
        set_interval(move || now.set(js_sys::Date::now()), Duration::from_secs(1));
        let post = move |path: &'static str, body: Option<serde_json::Value>| {
            let request = Request::post(path);
            widget.send(match body {
                Some(body) => request.json(&body).expect("json serialises"),
                None => request,
            })
        };
        let begin = move |phase: &'static str| {
            post(
                "/api/pomodoro/start",
                Some(serde_json::json!({ "phase": phase })),
            )
        };

        loaded_view(timer, move |timer| {
            // The answer is as of now; count down from here.
            let fetched = js_sys::Date::now();
            let Timer {
                phase,
                running,
                paused,
                left_ms,
                length_ms,
                finished,
            } = timer;
            let phase = phase.unwrap_or_default();
            let has_phase = !phase.is_empty();
            let left = move || {
                if running {
                    left_ms - (now.get() - fetched) as i64
                } else {
                    left_ms
                }
            };
            let over = move || has_phase && !paused && left() <= 0;
            // Idle, the ring is empty and the face says how long a focus is.
            let gone = move || match length_ms {
                0 => 0,
                whole => ((whole - left().max(0)) * 100 / whole).clamp(0, 100),
            };
            let face = move || clock(if has_phase { left() } else { FOCUS_MS });
            let words = {
                let phase = phase.clone();
                move || match (phase.as_str(), paused, over()) {
                    ("", _, _) => "Ready when you are",
                    ("focus", _, true) => "Focus done. Take a break",
                    ("break", _, true) => "Break over. Back to it",
                    ("focus", true, _) => "Focus, paused",
                    ("break", true, _) => "Break, paused",
                    ("focus", false, _) => "Focus",
                    _ => "Break",
                }
            };
            let tomatoes = (0..finished.min(8)).map(|_| "●").collect::<String>();
            let idle_or_over = move || !has_phase || over();
            view! {
                <div class="pomodoro" data-phase=phase.clone()>
                    <div
                        class="pomodoro__ring"
                        class:pomodoro__ring--break=phase == "break"
                        style=move || format!("--share: {}%", gone())
                    >
                        <span class="pomodoro__left" data-left=move || left().max(0)>{face}</span>
                    </div>
                    <div class="pomodoro__side">
                        <p class="pomodoro__words">{words}</p>
                        <p class="w-quiet" title="Focuses finished">
                            <span class="pomodoro__tomatoes">{tomatoes}</span>
                            {format!(" {finished} finished")}
                        </p>
                        <div class="w-row">
                            <Show when=idle_or_over>
                                <button class="w-button" disabled=move || read_only.get() on:click=move |_| begin("focus")>
                                    "Focus"
                                </button>
                                <button class="w-button w-button--quiet" disabled=move || read_only.get() on:click=move |_| begin("break")>
                                    "Break"
                                </button>
                            </Show>
                            <Show when=move || running && !over()>
                                <button class="w-button w-button--quiet" disabled=move || read_only.get() on:click=move |_| post("/api/pomodoro/pause", None)>
                                    "Pause"
                                </button>
                            </Show>
                            <Show when=move || paused>
                                <button class="w-button" disabled=move || read_only.get() on:click=move |_| post("/api/pomodoro/resume", None)>
                                    "Resume"
                                </button>
                            </Show>
                            <Show when=move || has_phase && !over()>
                                <button
                                    class="w-button w-button--quiet"
                                    disabled=move || read_only.get()
                                    on:click=move |_| widget.send(Request::delete("/api/pomodoro"))
                                >
                                    "Stop"
                                </button>
                            </Show>
                        </div>
                    </div>
                </div>
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_rounds_up_to_the_second_and_stops_at_zero() {
        assert_eq!(clock(25 * 60 * 1000), "25:00");
        assert_eq!(clock(61_001), "01:02");
        assert_eq!(clock(1), "00:01");
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(-5_000), "00:00");
    }
}
