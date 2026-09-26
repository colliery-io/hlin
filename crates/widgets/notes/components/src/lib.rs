//! The note's components: a sticky note, and a way to rewrite it.
//!
//! Everything shared with the other widgets (fetching again when told,
//! writes and their refusals) is `hlin-widget-ui`'s; what is here is
//! the note, the draft, and what happens when somebody else saved first.
//!
//! The draft lives outside the fetched note, so another person's edit arriving
//! mid-sentence redraws the note without losing a word of the draft. If their
//! edit landed first, the widget refuses this one; this then shows their
//! text beside the draft, and offers to save over it knowingly. A draft also
//! outlives a Hlin panel being scrolled away: it is handed to the shell on
//! `suspend` and taken back from `restored`. Its own page is never suspended.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The widget's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// The longest a note may be; the widget's own rule, repeated so the counter
/// can say so before the widget has to.
const LONGEST: usize = 500;

#[derive(Debug, Clone, Deserialize)]
struct Note {
    text: String,
    revision: u64,
    by: Option<String>,
}

/// A draft being written, as the shell keeps it while the panel is scrolled
/// away: the revision it was started from, a line break, and the text.
fn kept(revision: u64, draft: &str) -> Vec<u8> {
    format!("{revision}\n{draft}").into_bytes()
}

/// Back from [`kept`], if it is one.
fn from_kept(bytes: &[u8]) -> Option<(u64, String)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (revision, draft) = text.split_once('\n')?;
    Some((revision.parse().ok()?, draft.to_string()))
}

/// The whole widget.
#[component]
pub fn Notes() -> impl IntoView {
    let widget = use_widget();
    let note = widget.load::<Note>(|| Request::get("/api/note"));
    let read_only = widget.read_only();
    // The revision the draft was started from, while there is a draft;
    // and the draft, kept by the shell when the panel is scrolled far
    // enough away to be unmounted (HLIN-S-0007, *Budget*). If somebody
    // saved in the meantime, the restored draft is shown beside their
    // text, as it would have been had the panel never left.
    let restored = widget.restored().and_then(|bytes| from_kept(&bytes));
    let editing = RwSignal::new(restored.as_ref().map(|(revision, _)| *revision));
    let draft = RwSignal::new(restored.map(|(_, text)| text).unwrap_or_default());
    widget.on_suspend(move || {
        editing
            .get_untracked()
            .map(|revision| draft.with_untracked(|text| kept(revision, text)))
    });

    let save = move |revision: u64| {
        widget.send_then(
            Request::put("/api/note")
                .json(&serde_json::json!({ "text": draft.get_untracked(), "revision": revision }))
                .expect("a string serialises"),
            move |happened| {
                if happened {
                    editing.set(None);
                }
            },
        );
    };

    let drawn = loaded_view(note, move |note| {
        let by = note.by.clone().map_or_else(
            || "Nobody has edited it yet.".to_string(),
            |by| format!("Last edited by {by}"),
        );
        let current = note.clone();
        let mode = move || match editing.get() {
            None => {
                let empty = current.text.is_empty();
                let text = if empty {
                    "(empty)".to_string()
                } else {
                    current.text.clone()
                };
                let start_editing = {
                    let current = current.clone();
                    move |_| {
                        draft.set(current.text.clone());
                        editing.set(Some(current.revision));
                    }
                };
                view! {
                    <p class="note__text" class:note__text--empty=empty>{text}</p>
                    <div class="w-row">
                        <p class="w-quiet note__by">{by.clone()}</p>
                        <button
                            class="w-button w-button--quiet"
                            disabled=move || read_only.get()
                            on:click=start_editing
                        >
                            "Edit"
                        </button>
                    </div>
                }
                .into_any()
            }
            Some(started) => {
                // Somebody saved while this draft was being written: show
                // what the note says now, and save over it only if asked.
                let overtaken = (current.revision != started).then(|| {
                    let theirs = current.text.clone();
                    let who = current.by.clone().unwrap_or_else(|| "Someone".to_string());
                    let latest = current.revision;
                    view! {
                        <div class="note__theirs">
                            <p class="w-quiet">{format!("{who} changed it to:")}</p>
                            <p class="note__text">{theirs}</p>
                            <button
                                class="w-button w-button--quiet"
                                disabled=move || read_only.get()
                                on:click=move |_| save(latest)
                            >
                                "Save mine over it"
                            </button>
                        </div>
                    }
                });
                let stale = current.revision != started;
                view! {
                    {overtaken}
                    <textarea
                        class="w-input note__draft"
                        aria-label="Note"
                        maxlength=LONGEST.to_string()
                        prop:value=draft.get_untracked()
                        on:input=move |event| draft.set(event_target_value(&event))
                    ></textarea>
                    <div class="w-row">
                        <p class="w-quiet note__count">
                            {move || format!("{} / {LONGEST}", draft.with(|text| text.chars().count()))}
                        </p>
                        <button
                            class="w-button w-button--quiet"
                            on:click=move |_| editing.set(None)
                        >
                            "Cancel"
                        </button>
                        <button
                            class="w-button"
                            disabled=move || read_only.get() || stale
                            on:click=move |_| save(started)
                        >
                            "Save"
                        </button>
                    </div>
                }
                .into_any()
            }
        };
        view! { <div class="note" data-revision=note.revision>{mode}</div> }
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
    fn a_draft_survives_being_scrolled_away_and_back() {
        let draft = "Stand-up moves to\n10:15";
        assert_eq!(from_kept(&kept(7, draft)), Some((7, draft.to_string())));
    }

    #[test]
    fn kept_state_that_is_not_a_draft_is_ignored() {
        assert_eq!(from_kept(b"seven\ntext"), None);
        assert_eq!(from_kept(b"7"), None);
        assert_eq!(from_kept(&[0xff, 0xfe]), None);
    }
}
