//! The sample platform's annotations panel: notes pinned to moments, drawn by
//! a module built with the SDK (HLIN-T-0071).
//!
//! The smallest module that does everything a module is for, and so the one
//! the browser tests prove a real module against:
//!
//! - **It follows the time picker.** The shell's `context` carries the
//!   surface's range; the module reads the notes inside it, and reads again
//!   whenever the range moves.
//! - **It reads and writes as the viewer.** Both go through the shell with
//!   `fetch`, to this platform's own API, and arrive with the viewer's
//!   identity, bound to the request on a write. The platform's answer names
//!   who it thinks is asking, and the module shows it.
//! - **It says `changed` after a write,** so the same panel in anybody else's
//!   browser reads again, and never shows a note the platform did not keep:
//!   whatever the write's answer, the notes are read again.
//!
//! Nothing here knows it is in a frame; `hlin-module` does. Built with Trunk
//! into `dist/`, which the platform serves under its `assets` prefix; `angreal
//! demo up` builds it.

use hlin_module::hlin_bridge::{Context, Refusal};
use hlin_module::{BridgeError, Module, Reply, Request};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

/// The panel this module draws, as the manifest names it.
const PANEL: &str = "annotations";

/// Where the notes are, on this platform.
const PATH: &str = "/api/module/annotations";

/// Where a note is pinned: beneath the prefix the platform lets modules write.
const PIN: &str = "/api/module/annotations/pin";

/// What the shell logs as the kit this module was drawn with.
const KIT: &str = concat!(
    "leptos (hlin-sample-platform-module@",
    env!("CARGO_PKG_VERSION"),
    ")"
);

/// One note, as the platform describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Note {
    id: u64,
    at_millis: i64,
    text: String,
    author: String,
}

/// The platform's answer to a read.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Notes {
    /// Who the platform thinks is asking.
    viewer: String,
    annotations: Vec<Note>,
}

/// What the list shows.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    Loading,
    Notes(Notes),
    Refused(String),
}

fn main() {
    console_error_panic_hook::set_once();
    // Not Leptos's `spawn_local`: its executor starts when something is
    // mounted, and nothing is until `init` has arrived.
    wasm_bindgen_futures::spawn_local(async {
        let module = hlin_module::connect().await;
        let drawn = module.clone();
        leptos::mount::mount_to_body(move || view! { <Annotations module=drawn /> });
        module.ready(Some(KIT));
    });
    offer_spin();
}

/// `window.annotations.spin(ms)`: holds the frame's main thread for `ms`, in
/// a task of its own, as a module that blocks would. What the browser test of
/// the SDK's long-task warning asks for (HLIN-T-0090); nothing in the module
/// calls it. A task of its own, not the caller's, because a debugger's
/// evaluation is not a task the page's own observers see.
fn offer_spin() {
    use leptos::wasm_bindgen::JsValue;
    use leptos::wasm_bindgen::closure::Closure;

    let spin = Closure::<dyn Fn(f64)>::new(|millis: f64| {
        set_timeout(
            move || {
                let until = js_sys::Date::now() + millis;
                while js_sys::Date::now() < until {
                    // Nothing: that is the point.
                }
            },
            std::time::Duration::ZERO,
        );
    });
    let hooks = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&hooks, &JsValue::from_str("spin"), spin.as_ref());
    let _ = js_sys::Reflect::set(&js_sys::global(), &JsValue::from_str("annotations"), &hooks);
    // Lives as long as the frame does.
    spin.forget();
}

/// How close to now a range must end to be read as running up to the present.
const LIVE_SLACK_MILLIS: i64 = 60_000;

/// The read for the surface's range: the notes inside it, or every note when
/// the surface has no range.
///
/// A range that ends at about `now` is "the last hour", not an hour that has
/// finished: the shell resolves it to milliseconds once, so a note pinned a
/// moment later would fall just after its end and never be shown. Such a
/// range is read with no end at all.
fn read_for(context: &Context, now_millis: i64) -> Request {
    let request = Request::get(PATH);
    match context.time_range {
        Some(range) if range.to_millis >= now_millis - LIVE_SLACK_MILLIS => {
            request.query(format!("from={}", range.from_millis))
        }
        Some(range) => request.query(format!("from={}&to={}", range.from_millis, range.to_millis)),
        None => request,
    }
}

/// The range, in words and whole minutes.
fn range_words(context: &Context) -> (String, Option<i64>) {
    match context.time_range {
        Some(range) => {
            let minutes = (range.to_millis - range.from_millis) / 60_000;
            (format!("The last {minutes} minutes"), Some(minutes))
        }
        None => ("All time".to_string(), None),
    }
}

/// A successful answer's body, or what to tell the person, in the platform's
/// words when it was the platform that said no.
fn outcome(reply: Result<Reply, BridgeError>) -> Result<hlin_module::Answer, String> {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match reply {
        Ok(Reply::Answered(answer)) if answer.is_success() => Ok(answer),
        Ok(Reply::Answered(answer)) => Err(match answer.json::<Message>() {
            Ok(said) => said.message,
            Err(_) => format!("The platform said no (status {}).", answer.status),
        }),
        Ok(Reply::Refused(refused)) => Err(match refused.refusal {
            Refusal::ReadOnly => "This surface is read-only, so nothing is sent.".to_string(),
            Refusal::TooMany => "Too much at once. Try again in a moment.".to_string(),
            _ => "Hlin did not send this to the platform.".to_string(),
        }),
        Err(_) => Err("The panel closed before the platform answered.".to_string()),
    }
}

/// The time of day a note was pinned, as `HH:MM:SS` in the viewer's zone.
fn clock(at_millis: i64) -> String {
    let date = js_sys::Date::new(&(at_millis as f64).into());
    format!(
        "{:02}:{:02}:{:02}",
        date.get_hours(),
        date.get_minutes(),
        date.get_seconds()
    )
}

#[component]
fn Annotations(module: Module) -> impl IntoView {
    let context = module.context();
    let changes = module.changes();
    let read_only = module.read_only();
    let module = StoredValue::new(module);

    let shown = RwSignal::new(Shown::Loading);
    let reloads = RwSignal::new(0_u64);
    let problem = RwSignal::new(None::<String>);
    let draft = RwSignal::new(String::new());
    let sending = RwSignal::new(false);

    // A `changed` for this panel, by its sequence number, so two in a row are
    // still two reasons to read again.
    let changed_here = Memo::new(move |_| {
        changes.with(|change| {
            change
                .as_ref()
                .filter(|change| change.panel == PANEL)
                .map(|change| change.seq)
        })
    });

    // Read now, and again whenever the range moves, a change is relayed, or
    // this module wrote. Only the latest answer is shown.
    let latest = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let asked = read_for(&context.get(), js_sys::Date::now() as i64);
        let _ = changed_here.get();
        reloads.track();
        latest.update_value(|count| *count += 1);
        let mine = latest.get_value();
        spawn_local(async move {
            let reply = module.get_value().fetch(asked).await;
            if latest.get_value() != mine {
                return;
            }
            shown.set(match outcome(reply) {
                Ok(answer) => match answer.json::<Notes>() {
                    Ok(notes) => Shown::Notes(notes),
                    Err(_) => Shown::Refused(
                        "The platform answered with something unreadable.".to_string(),
                    ),
                },
                Err(words) => Shown::Refused(words),
            });
            // When the first content was drawn, for the load-time measurement
            // (NFR-1.1): the epoch clock, which the shell's page shares.
            if let Some(body) = document().body()
                && body.get_attribute("data-drawn-at").is_none()
            {
                let _ = body.set_attribute("data-drawn-at", &js_sys::Date::now().to_string());
            }
        });
    });

    let pin = move || {
        let text = draft.get_untracked();
        if text.trim().is_empty() || sending.get_untracked() {
            return;
        }
        let request = match Request::post(PIN).json(&serde_json::json!({ "text": text })) {
            Ok(request) => request,
            Err(_) => return,
        };
        let attempt = module.with_value(|module| module.attempt(request));
        sending.set(true);
        spawn_local(async move {
            match outcome(attempt.send().await) {
                Ok(_) => {
                    problem.set(None);
                    draft.set(String::new());
                    // The shell never infers a change from a write; the
                    // module says so, and the shell tells this platform's
                    // other modules, here and in other browsers.
                    module.with_value(|module| module.changed(PANEL, Default::default()));
                }
                Err(words) => problem.set(Some(words)),
            }
            sending.set(false);
            reloads.update(|count| *count += 1);
        });
    };

    view! {
        <p class="quiet">
            <span id="range" data-minutes=move || range_words(&context.get()).1.map(|m| m.to_string()).unwrap_or_default()>
                {move || range_words(&context.get()).0}
            </span>
            {move || match shown.get() {
                Shown::Notes(notes) => view! {
                    ", read as " <span id="viewer">{notes.viewer}</span>
                }.into_any(),
                _ => ().into_any(),
            }}
        </p>
        <form on:submit=move |event| {
            event.prevent_default();
            pin();
        }>
            <input
                id="note"
                placeholder="Pin a note to now"
                maxlength="280"
                prop:value=move || draft.get()
                on:input=move |event| draft.set(event_target_value(&event))
                disabled=move || read_only.get()
            />
            <button id="pin" type="submit" disabled=move || read_only.get() || sending.get()>
                "Pin"
            </button>
        </form>
        {move || problem.get().map(|words| view! { <p class="refusal" id="problem">{words}</p> })}
        {move || match shown.get() {
            Shown::Loading => view! { <p class="quiet">"Loading…"</p> }.into_any(),
            Shown::Refused(words) => view! { <p class="refusal">{words}</p> }.into_any(),
            Shown::Notes(notes) if notes.annotations.is_empty() => {
                view! { <p class="quiet" id="empty">"Nothing pinned in this range."</p> }.into_any()
            }
            Shown::Notes(notes) => view! {
                <ul id="notes">
                    {notes.annotations.into_iter().map(|note| view! {
                        <li data-id=note.id>
                            <span class="text">{note.text}</span>
                            " "
                            <span class="who">{format!("{}, {}", note.author, clock(note.at_millis))}</span>
                        </li>
                    }).collect_view()}
                </ul>
            }.into_any(),
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hlin_module::hlin_bridge::TimeRange;

    fn over(minutes: i64) -> Context {
        Context {
            time_range: Some(TimeRange {
                from_millis: 1_000_000,
                to_millis: 1_000_000 + minutes * 60_000,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn the_read_asks_for_the_surfaces_range() {
        let end = 1_000_000 + 15 * 60_000;
        // Long finished: that window exactly.
        let asked = read_for(&over(15), end + 10 * 60_000);
        assert_eq!(asked.path(), PATH);
        assert_eq!(
            asked,
            Request::get(PATH).query(format!("from=1000000&to={end}"))
        );
        // Ending about now: running on into the present.
        assert_eq!(
            read_for(&over(15), end + 5_000),
            Request::get(PATH).query("from=1000000")
        );
        assert_eq!(
            range_words(&over(15)),
            ("The last 15 minutes".to_string(), Some(15))
        );
        assert_eq!(range_words(&Context::default()).1, None);
    }

    #[test]
    fn the_platforms_refusal_is_shown_in_its_own_words() {
        let answer = hlin_module::Answer {
            status: 422,
            headers: Default::default(),
            body: br#"{"message":"A note needs some text."}"#.to_vec(),
        };
        assert_eq!(
            outcome(Ok(Reply::Answered(answer))).unwrap_err(),
            "A note needs some text."
        );
    }

    #[test]
    fn a_notes_answer_is_read() {
        let notes: Notes = serde_json::from_str(
            r#"{"viewer":"Ada","platform":"orebank","annotations":[{"id":1,"at_millis":5,"text":"x","author":"Ada"}]}"#,
        )
        .unwrap();
        assert_eq!(notes.viewer, "Ada");
        assert_eq!(notes.annotations[0].text, "x");
    }
}
