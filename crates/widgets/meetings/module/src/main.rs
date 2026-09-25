//! The meetings module: today's meetings in your own time zone, which one is
//! on now, and your answer to each.
//!
//! The widget sends instants; the browser knows where it is, so times are
//! shown in its zone, and "now" and "next" follow its clock, checked every
//! half minute.

use std::time::Duration;

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Meeting {
    id: String,
    title: String,
    room: String,
    starts: String,
    ends: String,
    with: Vec<String>,
    answer: Option<String>,
}

/// The answers there are, and how each is written on its button.
const ANSWERS: &[(&str, &str)] = &[("going", "Going"), ("maybe", "Maybe"), ("declined", "No")];

/// Where a meeting is, relative to now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Over,
    Now,
    Next,
    Later,
}

impl When {
    fn class(self) -> &'static str {
        match self {
            Self::Over => "meeting meeting--over",
            Self::Now => "meeting meeting--now",
            Self::Next => "meeting meeting--next",
            Self::Later => "meeting",
        }
    }
}

/// Each meeting's place relative to `now`, given as (start, end) in
/// milliseconds: over, on now, the next to start, or later than that.
fn placed(spans: &[(f64, f64)], now: f64) -> Vec<When> {
    let next = spans.iter().position(|(starts, _)| *starts > now);
    spans
        .iter()
        .enumerate()
        .map(|(at, (starts, ends))| {
            if *ends <= now {
                When::Over
            } else if *starts <= now {
                When::Now
            } else if Some(at) == next {
                When::Next
            } else {
                When::Later
            }
        })
        .collect()
}

fn millis(at: &str) -> f64 {
    js_sys::Date::parse(at)
}

/// `HH:MM` in the browser's own time zone.
fn local_time(at: &str) -> String {
    let date = js_sys::Date::new(&js_sys::JsString::from(at).into());
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

fn main() {
    start("meetings", |widget: Widget| {
        let meetings = widget.load::<Vec<Meeting>>(|| Request::get("/api/meetings"));
        let read_only = widget.read_only();
        let now = RwSignal::new(js_sys::Date::now());
        set_interval(
            move || now.set(js_sys::Date::now()),
            Duration::from_secs(30),
        );

        loaded_view(meetings, move |meetings| {
            if meetings.is_empty() {
                return view! { <p class="w-quiet">"Nothing on today."</p> }.into_any();
            }
            let spans: Vec<(f64, f64)> = meetings
                .iter()
                .map(|meeting| (millis(&meeting.starts), millis(&meeting.ends)))
                .collect();
            let places = Memo::new(move |_| placed(&spans, now.get()));
            let items = meetings
                .into_iter()
                .enumerate()
                .map(|(at, meeting)| {
                    let when = move || places.with(|places| places[at]);
                    let over = move || when() == When::Over;
                    let time = format!(
                        "{}–{}",
                        local_time(&meeting.starts),
                        local_time(&meeting.ends)
                    );
                    let answers = ANSWERS
                        .iter()
                        .map(|(answer, words)| {
                            let mine = meeting.answer.as_deref() == Some(*answer);
                            let path = format!("/api/meetings/{}/answer", meeting.id);
                            view! {
                                <button
                                    class="w-button meeting__answer"
                                    class:w-button--quiet=!mine
                                    aria-pressed=mine.to_string()
                                    disabled=move || read_only.get() || over()
                                    on:click=move |_| {
                                        widget.send(
                                            Request::put(path.clone())
                                                .json(&serde_json::json!({ "answer": answer }))
                                                .expect("an answer serialises"),
                                        )
                                    }
                                >
                                    {*words}
                                </button>
                            }
                        })
                        .collect_view();
                    view! {
                        <li class=move || when().class() data-meeting=meeting.id.clone()>
                            <span class="meeting__time">{time}</span>
                            <span class="meeting__what">
                                <b>{meeting.title}</b>
                                <span class="w-quiet">
                                    {format!("{} · with {}", meeting.room, meeting.with.join(", "))}
                                </span>
                            </span>
                            <span class="meeting__answers">{answers}</span>
                        </li>
                    }
                })
                .collect_view();
            view! { <ul class="w-list meetings">{items}</ul> }.into_any()
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meeting_is_over_on_now_next_or_later() {
        let spans = [(0.0, 10.0), (20.0, 30.0), (40.0, 50.0), (60.0, 70.0)];
        assert_eq!(
            placed(&spans, 25.0),
            [When::Over, When::Now, When::Next, When::Later]
        );
        assert_eq!(placed(&spans, 35.0)[2], When::Next);
        assert!(placed(&spans, 99.0).iter().all(|when| *when == When::Over));
    }
}
