//! The bookmarks' components: the team's links, newest first, and a line to
//! add one.
//!
//! Mounted twice, unchanged: at the root of the bookmarks' own origin by
//! `hlin-widget-bookmarks-ui`, and in a Hlin panel by
//! `hlin-widget-bookmarks-module`. Everything shared with the other widgets
//! (fetching again when told, writes and their refusals) is
//! `hlin-widget-ui`'s; what is here is what the bookmarks draw and ask for.

use hlin_widget_ui::{Request, loaded_view, use_widget};
use leptos::prelude::*;
use serde::Deserialize;

/// The bookmarks' own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// The list, as the widget describes it to this viewer.
#[derive(Debug, Clone, Deserialize)]
struct Shelf {
    links: Vec<Shown>,
    room: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct Shown {
    id: u64,
    url: String,
    title: String,
    host: String,
    added_by: String,
    removable: bool,
}

/// The whole widget.
#[component]
pub fn Bookmarks() -> impl IntoView {
    let widget = use_widget();
    let shelf = widget.load::<Shelf>(|| Request::get("/api/bookmarks"));
    let read_only = widget.read_only();
    let url = RwSignal::new(String::new());
    let title = RwSignal::new(String::new());

    let add = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let link = url.get_untracked();
        if link.trim().is_empty() {
            return;
        }
        let named = title.get_untracked();
        widget.send_then(
            Request::post("/api/bookmarks")
                .json(&serde_json::json!({ "url": link, "title": named }))
                .expect("strings serialise"),
            move |happened| {
                // Cleared only if it still says what was sent: anything
                // typed while the link was on its way is kept.
                if happened && url.get_untracked() == link && title.get_untracked() == named {
                    url.set(String::new());
                    title.set(String::new());
                }
            },
        );
    };

    let list = loaded_view(shelf, move |shelf| {
        let links = shelf
            .links
            .into_iter()
            .map(|link| {
                let path = format!("/api/bookmarks/{}", link.id);
                let remove = link.removable.then(|| {
                    view! {
                        <button
                            class="bookmarks__remove"
                            aria-label=format!("Remove {}", link.title)
                            title="Remove"
                            disabled=move || read_only.get()
                            on:click=move |_| widget.send(Request::delete(path.clone()))
                        >
                            "×"
                        </button>
                    }
                });
                view! {
                    <li class="bookmarks__link" data-bookmark=link.id>
                        <a href=link.url.clone() target="_blank" rel="noopener noreferrer">{link.title}</a>
                        <span class="bookmarks__host" title=format!("Added by {}", link.added_by)>{link.host}</span>
                        {remove}
                    </li>
                }
            })
            .collect_view();
        let room = match shelf.room {
            0 => "The list is full.".to_string(),
            1 => "Room for one more.".to_string(),
            n => format!("Room for {n} more."),
        };
        view! {
            <ul class="w-list bookmarks__list">{links}</ul>
            <p class="w-quiet">{room}</p>
        }
    });

    view! {
        <style>{STYLE}</style>
        {list}
        <form class="bookmarks__add" on:submit=add>
            <input
                class="w-input"
                type="url"
                aria-label="Link"
                placeholder="https://…"
                prop:value=move || url.get()
                on:input=move |event| url.set(event_target_value(&event))
                disabled=move || read_only.get()
            />
            <input
                class="w-input"
                aria-label="Title"
                placeholder="Title (optional)"
                maxlength="80"
                prop:value=move || title.get()
                on:input=move |event| title.set(event_target_value(&event))
                disabled=move || read_only.get()
            />
            <button class="w-button" type="submit" disabled=move || read_only.get()>
                "Add"
            </button>
        </form>
    }
}
