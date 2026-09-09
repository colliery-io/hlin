//! The surface a person looks at, and composes.
//!
//! Everything with a decision in it lives elsewhere: `state` holds what the
//! browser believes about the panels, `draft` holds the layout being edited,
//! `grid` holds where things sit. This module is the wiring between those and
//! the DOM, and it is deliberately thin, because none of it can be tested
//! without a browser.

use std::collections::BTreeMap;

use crate::pack::Drawer;
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use hlin_manifest::envelope::Choice;
use hlin_stream::layout::{CatalogPanel, CatalogPlatform, Placement};
use hlin_stream::{Frame, ParamsRequest, TimeRange};
use hlin_view::{Kind, plan};
use leptos::ev::PointerEvent;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::draft::LayoutDraft;
use crate::grid;
use crate::state::SurfaceState;

/// How long a lost stream is merely stale before it is called unreachable.
///
/// Replaced by whatever `/api/config` says as soon as that answers; this is
/// what the browser uses in the moment before it has been told.
const GRACE: i64 = 30;

/// The time ranges the picker offers without anyone typing a date.
const PRESETS: [(&str, i64); 4] = [("15m", 900), ("1h", 3600), ("6h", 21_600), ("24h", 86_400)];

/// What the viewer is doing with the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Looking at it.
    Watching,
    /// Changing it.
    Composing,
}

/// A gesture in progress.
///
/// Held while a pointer is down, so a pointer move knows what it is moving and
/// from where. Cleared on pointer up, which is also when the layout is written.
#[derive(Debug, Clone)]
struct Gesture {
    instance: String,
    kind: GestureKind,
    /// Where the pointer went down.
    from: (f64, f64),
    /// Where the panel was when it did.
    origin: Placement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GestureKind {
    Move,
    Resize,
}

/// The whole thing.
///
/// Generic over the design pack, which is the only thing a consumer has to
/// supply. The pack is erased into a [`Drawer`] on the first line, so nothing
/// below this component is generic and nothing below it knows which design
/// system is drawing.
#[component]
pub fn App<P>(
    /// What draws the panels. The only thing a consumer supplies.
    pack: P,
) -> impl IntoView
where
    P: hlin_view::DesignPack + Send + Sync + 'static,
    P::View: IntoView + 'static,
{
    let drawer = Drawer::of(pack);

    let (surface, set_surface) = signal(SurfaceState::new());
    let (draft, set_draft) = signal(LayoutDraft::empty());
    let (catalog, set_catalog) = signal(Vec::<CatalogPlatform>::new());
    // What each control will accept, by panel reference and control id.
    let (choices, set_choices) = signal(BTreeMap::<(String, String), Vec<Choice>>::new());
    let (mode, set_mode) = signal(Mode::Watching);
    let (connected, set_connected) = signal(false);
    let (grace, set_grace) = signal(GRACE);

    // Whether this shell refuses every write. False until `/api/config`
    // answers, and that way round on purpose: a browser that guessed read-only
    // and was wrong hides composition from a shell that offers it, which is
    // silent, while guessing writable and being wrong costs one 403 nobody
    // reaches because the surface has not loaded yet either.
    let (read_only, set_read_only) = signal(false);
    let (trouble, set_trouble) = signal(Option::<String>::None);
    let (chosen_range, set_chosen_range) = signal(3600i64);
    let (custom, set_custom) = signal(Option::<TimeRange>::None);
    let (search, set_search) = signal(String::new());
    let (gesture, set_gesture) = signal(Option::<Gesture>::None);

    // Bumped whenever the surface must be re-subscribed: when the layout is
    // first known, and after every write, because a write can add or remove
    // panels and the running stream was built from the panels the layout had.
    let (revision, set_revision) = signal(0u32);
    let (cell, set_cell) = signal(0.0f64);

    // How many layout writes have been issued. Only the newest one's answer is
    // allowed to replace the draft.
    let (writes, set_writes) = signal(0u32);

    // How many are still in flight.
    //
    // A gesture changes the draft at once and writes afterwards, which is what
    // makes composition feel immediate — and it means that between the two the
    // surface on screen is a promise rather than a fact. Leave in that gap and
    // the promise is broken silently: the browser cancels the request on
    // navigation, the shell never hears about the change, and the panel is back
    // when the page returns. So the surface says when it is still owed a write.
    let (writing, set_writing) = signal(0u32);

    let stage: NodeRef<html::Main> = NodeRef::new();

    // What the shell says about itself, and what there is to compose from.
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(config) = crate::api::config().await {
                set_grace.set(config.stream_loss_grace_seconds);
                set_read_only.set(config.read_only);
            }
            // A link to a surface names the layout; a bare visit gets the
            // principal's own. Either way the address bar ends up naming what
            // is on screen, so the link can be sent to somebody.
            let asked = requested_layout();
            let fetched = match &asked {
                Some(id) => crate::api::layout(id).await,
                None => crate::api::home().await,
            };

            match fetched {
                Ok(document) => {
                    if asked.is_none()
                        && let Some(id) = document.surface_id()
                    {
                        show_address(&format!("/s/{id}"));
                    }
                    set_draft.set(LayoutDraft::of(document));
                    set_revision.update(|revision| *revision += 1);
                }
                Err(reason) => set_trouble.set(Some(reason)),
            }
            if let Ok(platforms) = crate::api::catalog().await {
                // What each control will accept, asked for once. The catalog
                // says which controls exist; only the shell can reach the
                // platform that knows their values, so this is the browser
                // asking it to.
                //
                // Options are fetched by their own effect, below, because which
                // ones are wanted depends on what is on the surface and that
                // changes while a person composes.
                set_catalog.set(platforms);
            }
        });
    });

    // What a control will accept, for the panels this surface actually holds.
    //
    // Its own effect rather than part of the catalogue fetch, because the
    // answer changes while somebody composes: a panel added after the page
    // loaded needs its options too, and fetching everything upfront paid for
    // panels nobody had put anywhere and grew with the catalogue rather than
    // with what is on screen.
    //
    // Keyed on a *memo of the references* so it re-runs when the set of panels
    // changes and not on every pointer move of a drag — the draft changes
    // constantly during a gesture, and the same trap already cost the stream
    // effect a connection per frame.
    let references = Memo::new(move |_| {
        draft.with(|draft| {
            let mut references: Vec<String> = draft
                .panels()
                .iter()
                .map(|panel| panel.reference())
                .collect();
            references.sort();
            references.dedup();
            references
        })
    });

    Effect::new(move |_| {
        let wanted_for = references.get();
        let platforms = catalog.get();
        if wanted_for.is_empty() || platforms.is_empty() {
            return;
        }

        // Only what is not already known. A person adding a second panel from a
        // platform should not re-ask for the choices the first one fetched.
        let known = choices.with_untracked(|listed| {
            listed
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        });

        let mut missing = Vec::new();
        for platform in &platforms {
            for panel in &platform.panels {
                if !wanted_for.contains(&panel.reference) {
                    continue;
                }
                for control in &panel.controls {
                    if control.options.is_none() {
                        continue;
                    }
                    let key = (panel.reference.clone(), control.id.clone());
                    if known.contains(&key) {
                        continue;
                    }
                    missing.push((
                        key,
                        platform.id.clone(),
                        panel.key.clone(),
                        control.id.clone(),
                    ));
                }
            }
        }

        if missing.is_empty() {
            return;
        }

        leptos::task::spawn_local(async move {
            // Fanned out rather than awaited in turn: the shell answers each
            // from a different platform, so doing them one after another makes
            // a person wait for the sum of every platform's latency.
            //
            // Not cached in the shell, which was the other option on the table.
            // The shell fetches these with the *viewer's* credential and a
            // platform may legitimately answer two principals differently
            // (HLIN-A-0004), so a shared cache would be a leak and a correct
            // one would have to be keyed per principal — more machinery than a
            // value this cheap to fetch deserves.
            let fetched = futures::future::join_all(missing.into_iter().map(
                |(key, platform, panel, control)| async move {
                    (key, crate::api::options(&platform, &panel, &control).await)
                },
            ))
            .await;

            set_choices.update(|listed| listed.extend(fetched));
        });
    });

    // The stream, reopened whenever the layout's shape changes. The previous
    // `EventSource` is closed first, or a browser would hold one connection per
    // edit for as long as the page stayed open.
    let held: StoredValue<Option<web_sys::EventSource>> = StoredValue::new(None);
    Effect::new(move |_| {
        // Only the revision. Reading the draft here would track it, and the
        // draft changes on every pointer move of a drag — which closed and
        // reopened the stream tens of times per gesture, and left the browser
        // holding a connection it had just closed. The revision moves once per
        // write, which is exactly when the surface's panels can have changed.
        let _ = revision.get();
        let Some(surface_id) = draft.with_untracked(|draft| draft.surface_id().map(str::to_string))
        else {
            return;
        };

        held.update_value(|existing| {
            if let Some(source) = existing.take() {
                source.close();
            }
        });

        let applied = move |frame: Frame| {
            set_surface.update(|state| {
                state.apply(&frame);
            });
        };
        let lost = move || {
            set_connected.set(false);
            set_surface.update(|state| state.on_stream_lost(Utc::now()));
        };
        let opened = move || {
            set_connected.set(true);
            set_surface.update(|state| state.on_stream_restored());
        };

        if let Ok(source) =
            crate::stream::open(&format!("/api/stream/{surface_id}"), applied, lost, opened)
        {
            held.set_value(Some(source));
        }
    });

    // While the stream is gone, the browser decides for itself when "not
    // current" becomes "not coming".
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(1_000).await;
                set_surface.update(|state| {
                    state.on_tick(Utc::now(), Duration::seconds(grace.get_untracked()));
                });
            }
        });
    });

    // -- Writing ---------------------------------------------------------

    // One write, carrying the whole layout, issued when a gesture ends rather
    // than while it is in progress. The surface re-subscribes once the shell
    // has answered, so the stream is always built from what was stored.
    let save = move || {
        if !draft.with_untracked(|draft| draft.dirty() && draft.editable()) {
            return;
        }
        let document = draft.with_untracked(|draft| draft.document().clone());

        // Which write this is. The shell's answer replaces the whole draft, so
        // an older answer arriving after a newer one would put back what the
        // newer one changed: remove a panel while a rename is still in flight
        // and the panel comes back. Same rule the stream already follows for
        // frames, for the same reason.
        let ticket = writes.get_untracked() + 1;
        set_writes.set(ticket);
        set_writing.update(|count| *count += 1);

        leptos::task::spawn_local(async move {
            let settled = crate::api::save(&document).await;
            set_writing.update(|count| *count = count.saturating_sub(1));

            match settled {
                Ok(stored) => {
                    if writes.get_untracked() != ticket {
                        // A later write is in flight. Its answer is the one
                        // that describes the layout now.
                        return;
                    }
                    set_draft.update(|draft| draft.written(stored));
                    set_trouble.set(None);

                    // Deliberately no `set_revision` here any more. The shell
                    // reconciles a running surface against what was written
                    // rather than dropping it, so the stream already carries
                    // the new arrangement — and re-subscribing would throw away
                    // every panel's data to fetch the same values again, which
                    // is exactly the blink this pair of changes removes.
                    //
                    // The revision still moves when the *layout being watched*
                    // changes, which is the only thing it was ever really for.
                }
                Err(reason) => set_trouble.set(Some(reason)),
            }
        });
    };

    // -- The time picker --------------------------------------------------

    let apply_range = move |range: TimeRange| {
        let generation = {
            let mut next = 0;
            set_surface.update(|state| next = state.next_generation());
            next
        };
        let selections = draft.with_untracked(|draft| draft.selections());

        let request = ParamsRequest {
            protocol_version: hlin_stream::PROTOCOL_VERSION,
            generation,
            time_range: Some(range),
            selections,
        };
        let surface_id = draft.with_untracked(|draft| draft.surface_id().map(str::to_string));

        leptos::task::spawn_local(async move {
            if let Some(surface_id) = surface_id {
                let _ = crate::stream::set_params(&surface_id, &request).await;
            }
        });
    };

    let apply_preset = move |seconds: i64| {
        set_chosen_range.set(seconds);
        set_custom.set(None);
        let to = Utc::now();
        apply_range(TimeRange {
            from: to - Duration::seconds(seconds),
            to,
        });
    };

    // A parameter change that keeps whatever range is in force. Used when a
    // per-panel selection changes, and once when the surface first opens, so
    // the shell fetches the range the picker is showing rather than whatever a
    // platform defaults to when asked for no window at all.
    let reapply = move || match custom.get_untracked() {
        Some(range) => apply_range(range),
        None => {
            let to = Utc::now();
            apply_range(TimeRange {
                from: to - Duration::seconds(chosen_range.get_untracked()),
                to,
            })
        }
    };

    Effect::new(move |applied_for: Option<u32>| {
        let revision = revision.get();
        // Nothing to ask for until the layout is known, and asking again on
        // every write would restart the fan-out for a panel that just moved.
        if revision > 0 && applied_for != Some(revision) {
            reapply();
        }
        revision
    });

    // -- Gestures ---------------------------------------------------------

    // The measured width of one column, taken from the grid element itself, so
    // the arithmetic works at any window size without anything being hardcoded.
    let measure = move || {
        if let Some(element) = stage.get_untracked() {
            let width = element.get_bounding_client_rect().width();
            set_cell.set(width / hlin_stream::layout::COLUMNS as f64);
        }
    };

    let begin = move |event: PointerEvent, instance: String, kind: GestureKind| {
        if mode.get_untracked() != Mode::Composing {
            return;
        }
        event.prevent_default();
        measure();

        let Some(origin) = draft.with_untracked(|draft| draft.placement_of(&instance)) else {
            return;
        };

        set_gesture.set(Some(Gesture {
            instance,
            kind,
            from: (event.client_x() as f64, event.client_y() as f64),
            origin,
        }));
    };

    let during = move |event: PointerEvent| {
        let Some(active) = gesture.get_untracked() else {
            return;
        };
        let cell = cell.get_untracked();
        if cell <= 0.0 {
            return;
        }

        // Rows are square-ish rather than square: a row shorter than a column
        // makes a four-by-four panel read as a card rather than a tile.
        let row = cell * 0.75;
        let across = grid::cells(event.client_x() as f64 - active.from.0, cell);
        let down = grid::cells(event.client_y() as f64 - active.from.1, row);

        set_draft.update(|draft| match active.kind {
            GestureKind::Move => draft.move_to(
                &active.instance,
                grid::shifted(active.origin.x, across),
                grid::shifted(active.origin.y, down),
            ),
            GestureKind::Resize => draft.resize(
                &active.instance,
                grid::shifted(active.origin.w, across).max(1),
                grid::shifted(active.origin.h, down).max(1),
            ),
        });
    };

    let end = move |_: PointerEvent| {
        if gesture.get_untracked().is_none() {
            return;
        }
        set_gesture.set(None);
        save();
    };

    // A gesture is tracked on the window, not on the grid.
    //
    // Found by a browser test: dragging a panel's corner down to make it taller
    // takes the pointer below the grid within about a hundred pixels, and a
    // listener on the grid stops hearing about it there. The panel then froze
    // mid-gesture and never let go, because the pointer-up that would have
    // ended it landed on the document.
    //
    // Pointer capture was supposed to prevent that and does not survive here:
    // the element captured is inside the panel being redrawn, and a captured
    // element that is replaced releases the pointer. The window is always under
    // the pointer, so listening there needs nothing to survive anything.
    Effect::new(move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };

        let moved = Closure::<dyn Fn(web_sys::PointerEvent)>::new(during);
        let _ =
            window.add_event_listener_with_callback("pointermove", moved.as_ref().unchecked_ref());
        moved.forget();

        let released = Closure::<dyn Fn(web_sys::PointerEvent)>::new(end);
        let _ =
            window.add_event_listener_with_callback("pointerup", released.as_ref().unchecked_ref());
        released.forget();

        // A gesture the browser takes away, by a window losing focus or a
        // touch being cancelled, must not leave a panel stuck to the pointer.
        let cancelled = Closure::<dyn Fn(web_sys::PointerEvent)>::new(end);
        let _ = window
            .add_event_listener_with_callback("pointercancel", cancelled.as_ref().unchecked_ref());
        cancelled.forget();
    });

    // -- The view ---------------------------------------------------------

    let composing = move || mode.get() == Mode::Composing;
    let editable = move || draft.with(|draft| draft.editable());

    let styling = drawer.stylesheet();

    // Taken here, before the pack is moved into the closures that draw with it,
    // for the same reason the stylesheet is.
    let offered = drawer.offers().join(" ");

    view! {
        // This crate's own chrome: the grid, the panel frames, the toolbar.
        // Injected rather than left for the consumer to link, because the file
        // lives in their registry cache and they have no path to it — the
        // examples here linked it relative to the source tree, which works
        // nowhere else. A front end built by following the documentation
        // rendered every panel unstyled and stacked in a column.
        <style>{crate::APP_CSS}</style>

        // The pack's own styling, put on the page by the app that mounted it.
        // Nothing else has to know a design system was chosen.
        <style>{styling}</style>

        <header class="bar">
            <strong>"Hlin"</strong>
            <span class="tagline">{move || draft.with(|draft| draft.title().to_string())}</span>

            <div class="picker">
                {PRESETS.into_iter().map(|(label, seconds)| {
                    let active = move || custom.get().is_none() && chosen_range.get() == seconds;
                    view! {
                        <button class:active=active on:click=move |_| apply_preset(seconds)>
                            {label}
                        </button>
                    }
                }).collect_view()}
                <CustomRange on_apply=move |range| {
                    set_custom.set(Some(range));
                    apply_range(range);
                } />
            </div>

            <span class="applied" class:pending=move || !surface.with(SurfaceState::applied)>
                {move || if surface.with(SurfaceState::applied) { "applied" } else { "applying…" }}
            </span>

            // Only while there is something outstanding. A surface with nothing
            // owed should not carry a permanent badge saying so.
            <Show when=move || writing.get() != 0>
                <span class="saving">"saving…"</span>
            </Show>

            <span class="link" class:down=move || !connected.get()>
                {move || if connected.get() { "live" } else { "reconnecting" }}
            </span>

            // Absent rather than disabled where the shell writes nothing.
            // A disabled control says "not for you, not now"; there is no
            // later here, and no explanation that would make one appear.
            <Show when=move || !read_only.get()>
                <button
                    class="mode"
                    class:active=composing
                    disabled=move || !editable()
                    title=move || if editable() {
                        String::new()
                    } else {
                        "This layout belongs to someone else. Fork it to make changes".to_string()
                    }
                    on:click=move |_| set_mode.update(|mode| {
                        *mode = if *mode == Mode::Composing {
                            Mode::Watching
                        } else {
                            Mode::Composing
                        };
                    })
                >
                    {move || if composing() { "Done" } else { "Edit" }}
                </button>
            </Show>
        </header>

        {move || trouble.get().map(|reason| view! {
            <p class="trouble">{reason}</p>
        })}

        <div class="stage" class:composing=composing>
            <Show when=composing>
                <aside class="catalog">
                    <input
                        class="search"
                        placeholder="Search panels"
                        prop:value=move || search.get()
                        on:input=move |event| set_search.set(event_value(&event))
                    />
                    {move || {
                        let needle = search.get().to_lowercase();
                        catalog.get().into_iter().map(|platform| {
                            let matching: Vec<CatalogPanel> = platform.panels
                                .iter()
                                .filter(|panel| matches(panel, &needle))
                                .cloned()
                                .collect();
                            if matching.is_empty() {
                                return ().into_any();
                            }
                            let name = platform.name.clone().unwrap_or_else(|| platform.id.clone());
                            let reachable = platform.reachable;

                            view! {
                                    <section class="platform">
                                        <h3>
                                            {name.clone()}
                                            <Show when=move || !reachable>
                                                <span class="warn" title="not answering right now">"!"</span>
                                            </Show>
                                        </h3>
                                        {matching.into_iter().map(|panel| {
                                            let platform_id = platform.id.clone();
                                            let key = panel.key.clone();
                                            view! {
                                                <button
                                                    class="offer"
                                                    title=panel.description.clone().unwrap_or_default()
                                                    on:click=move |_| {
                                                        set_draft.update(|draft| draft.add(&platform_id, &key));
                                                        save();
                                                    }
                                                >
                                                    <span class="offer-title">{panel.title.clone()}</span>
                                                    <span class="offer-kind">{panel.kind.clone()}</span>
                                                </button>
                                            }
                                        }).collect_view()}
                                    </section>
                            }.into_any()
                        }).collect_view()
                    }}
                </aside>
            </Show>

            <main
                class="grid"
                node_ref=stage
                // What the mounted pack answers to, on the page rather than
                // only in a console warning. A component is the one thing about
                // a panel that looking at the panel cannot settle: a pack that
                // declines one draws the declared kind, which is exactly what a
                // pack that never heard of components draws. Anything asking
                // whether the seam works has to be able to tell those apart,
                // and a frontend built with a single pack has to be able to say
                // so rather than fail as though the seam were broken.
                data-offers=offered
                style:min-height=move || format!("{}px", (draft.with(|d| d.depth()).max(4)) * 72)
                // The gesture listeners are on the window, not here. See the
                // effect above.
            >
                {move || {
                    let panels = draft.with(|draft| draft.panels().to_vec());
                    if panels.is_empty() {
                        return view! { <Empty composing=composing() read_only=read_only.get() /> }.into_any();
                    }

                    panels.into_iter().map(|instance| {
                        let id = instance.id.clone();
                        let position = instance.position;
                        let known = id.clone().and_then(|id| surface.with(|state| state.panel(&id).cloned()));
                        let catalogued = catalogued(&catalog.get(), &instance.platform_id, &instance.panel_key);
                        let named = instance.id.clone().unwrap_or_default();
                        let reference = instance.reference();

                        // A panel the stream has never mentioned, on a stream
                        // that has stopped answering, is not loading. Nothing is
                        // coming, and a skeleton that never resolves tells a
                        // person the panel is slow when the shell is gone.
                        let abandoned = known.is_none()
                            && surface.with(|state| {
                                state.given_up(Utc::now(), Duration::seconds(grace.get()))
                            });

                        let drawn_state = match (&known, abandoned) {
                            (Some(panel), _) => state_name(panel.state),
                            (None, true) => "unavailable",
                            (None, false) => "waiting",
                        };

                        // What a component may change, and what it is set to.
                        // Built from the same two things the chrome's own
                        // controls read, so a pack drawing its own filter and
                        // the control beside it can never disagree.
                        let controls: Vec<hlin_view::Control> = catalogued
                            .as_ref()
                            .map(|entry| entry.controls.clone())
                            .unwrap_or_default()
                            .into_iter()
                            .map(|control| hlin_view::Control {
                                param: control.param,
                                label: control.label,
                                chosen: instance
                                    .selections
                                    .get(&control.id)
                                    .cloned()
                                    .unwrap_or_default(),
                                choices: choices
                                    .with(|listed| {
                                        listed
                                            .get(&(reference.clone(), control.id.clone()))
                                            .cloned()
                                    })
                                    .unwrap_or_default(),
                                id: control.id,
                            })
                            .collect();

                        // Where this panel's components send what a person did.
                        // Every intent lands on the handler the chrome already
                        // uses, which is the point: a design system gets the
                        // capabilities the shell has, and no others.
                        let emit = {
                            let who = named.clone();
                            hlin_view::Emit::to(move |intent| match intent {
                                hlin_view::Intent::Select { param, values } => {
                                    if who.is_empty() {
                                        return;
                                    }
                                    set_draft.update(|draft| {
                                        draft.set_selection(&who, &param, values)
                                    });
                                    save();
                                    reapply();
                                }
                                hlin_view::Intent::Range { from_millis, to_millis } => {
                                    let Some(range) = span(from_millis, to_millis) else {
                                        return;
                                    };
                                    // The picker is told as well as the shell.
                                    // A chart brushed to an hour that left the
                                    // bar still saying "24h" would be the
                                    // chrome lying about what is on screen.
                                    set_custom.set(Some(range));
                                    set_chosen_range.set(0);
                                    apply_range(range);
                                }
                            })
                        };

                        view! {
                            <section
                                class="panel"
                                class:dragging=move || gesture.get().is_some_and(|active| Some(&active.instance) == id.as_ref())
                                // Named and located in the markup, so a browser
                                // test can say "this panel moved" rather than
                                // parsing a style attribute for grid lines.
                                data-instance=named
                                data-panel=reference
                                data-x=position.x.to_string()
                                data-y=position.y.to_string()
                                data-w=position.w.to_string()
                                data-h=position.h.to_string()
                                data-state=drawn_state
                                style:grid-column=format!("{} / span {}", position.x + 1, position.w)
                                style:grid-row=format!("{} / span {}", position.y + 1, position.h)
                            >
                                <PanelHead
                                    instance=instance.clone()
                                    catalogued=catalogued.clone()
                                    composing=composing()
                                    on_grip=move |event, who| begin(event, who, GestureKind::Move)
                                    on_remove=move |who: String| {
                                        set_draft.update(|draft| draft.remove(&who));
                                        save();
                                    }
                                    on_rename=move |(who, title): (String, String)| {
                                        set_draft.update(|draft| draft.rename(&who, &title));
                                        save();
                                    }
                                    on_kind=move |(who, kind): (String, Option<String>)| {
                                        set_draft.update(|draft| draft.set_kind(&who, kind.as_deref()));
                                        save();
                                    }
                                    controls=controls.clone()
                                    on_select=move |(who, param, value): (String, String, String)| {
                                        let values = if value.is_empty() { vec![] } else { vec![value] };
                                        set_draft.update(|draft| draft.set_selection(&who, &param, values));
                                        save();
                                        reapply();
                                    }
                                />

                                <PanelBody
                                    drawer=drawer.clone()
                                    known=known
                                    abandoned=abandoned
                                    kind_override=instance.kind_override.clone()
                                    catalogued=catalogued
                                    controls=controls
                                    emit=emit
                                />

                                <Show when=move || composing()>
                                    <span
                                        class="corner"
                                        on:pointerdown={
                                            let who = instance.id.clone();
                                            move |event: PointerEvent| {
                                                if let Some(who) = who.clone() {
                                                    begin(event, who, GestureKind::Resize);
                                                }
                                            }
                                        }
                                    />
                                </Show>
                            </section>
                        }
                    }).collect_view().into_any()
                }}
            </main>
        </div>
    }
}

/// A surface with nothing on it.
#[component]
fn Empty(composing: bool, read_only: bool) -> impl IntoView {
    view! {
        <p class="empty">
            {if composing {
                "Pick a panel on the left to put it here."
            } else if read_only {
                // Telling somebody to press a button that is not there is
                // worse than saying nothing, and this is the one screen where
                // the difference between the two shells is visible.
                "Nothing on this surface yet."
            } else {
                "Nothing on this surface yet. Press Edit to compose one."
            }}
        </p>
    }
}

/// A panel's heading: its title, and, while composing, what can be done to it.
#[component]
fn PanelHead(
    instance: hlin_stream::layout::PanelInstanceDocument,
    catalogued: Option<CatalogPanel>,
    composing: bool,
    on_grip: impl Fn(PointerEvent, String) + 'static,
    on_remove: impl Fn(String) + 'static,
    on_rename: impl Fn((String, String)) + 'static,
    on_kind: impl Fn((String, Option<String>)) + 'static,
    on_select: impl Fn((String, String, String)) + 'static,
    /// The panel's controls, already carrying what each will accept and what
    /// it is set to, so the chrome and any component drawing its own filter
    /// are reading one thing rather than two.
    controls: Vec<hlin_view::Control>,
) -> impl IntoView {
    let Some(id) = instance.id.clone() else {
        // A panel the shell has not written yet: it has no identity to act on,
        // and it will have one within a request. Showing it inert beats not
        // showing it at all, which would make adding a panel look like nothing
        // happened.
        return view! {
            <header class="panel-head">
                <h2>{title_of(&instance, catalogued.as_ref())}</h2>
                <span class="pending-panel">"saving…"</span>
            </header>
        }
        .into_any();
    };

    let title = title_of(&instance, catalogued.as_ref());
    let kinds = catalogued
        .as_ref()
        .map(|panel| panel.available_kinds.clone())
        .unwrap_or_default();
    let default_kind = catalogued
        .as_ref()
        .map(|panel| panel.kind.clone())
        .unwrap_or_else(|| "raw".to_string());
    let chosen_kind = instance.kind_override.clone();

    if !composing {
        return view! {
            <header class="panel-head">
                <h2>{title}</h2>
            </header>
        }
        .into_any();
    }

    view! {
        <header class="panel-head editing">
            <span
                class="grip"
                title="Drag to move"
                on:pointerdown={
                    let id = id.clone();
                    move |event: PointerEvent| on_grip(event, id.clone())
                }
            >"⠿"</span>

            <input
                class="rename"
                prop:value=title
                on:change={
                    let id = id.clone();
                    move |event| on_rename((id.clone(), event_value(&event)))
                }
            />

            <select
                class="kind"
                on:change={
                    let id = id.clone();
                    move |event| {
                        let value = event_value(&event);
                        on_kind((id.clone(), if value.is_empty() { None } else { Some(value) }));
                    }
                }
            >
                <option value="" selected=chosen_kind.is_none()>
                    {format!("{default_kind} (default)")}
                </option>
                {kinds.into_iter().map(|kind| {
                    let selected = chosen_kind.as_deref() == Some(kind.as_str());
                    let label = kind.clone();
                    view! { <option value=kind selected=selected>{label}</option> }
                }).collect_view()}
            </select>

            {
                // One handler shared by every control on this panel, so a
                // panel with two of them still compiles: a closure that is
                // called from a loop cannot own what it calls.
                let on_select = std::rc::Rc::new(on_select);
                controls.into_iter().map(|control| {
                let value = control.value().unwrap_or_default().to_string();
                let id = id.clone();
                let param = control.id.clone();
                let on_select = on_select.clone();
                let label = control.label.clone();

                // A platform that listed what it accepts gets a menu; one that
                // listed nothing gets the text box this always was. Neither is
                // a fallback for the other — a platform may accept values it
                // never enumerated, and the text box is how a person reaches
                // them.
                if control.choices.is_empty() {
                    return view! {
                        <input
                            class="control"
                            placeholder=label.clone()
                            title=label
                            prop:value=value
                            on:change=move |event| {
                                on_select((id.clone(), param.clone(), event_value(&event)))
                            }
                        />
                    }.into_any();
                }

                let offered = control.choices.clone();
                view! {
                    <select
                        class="control"
                        title=label.clone()
                        prop:value=value.clone()
                        on:change=move |event| {
                            on_select((id.clone(), param.clone(), event_value(&event)))
                        }
                    >
                        // An empty first entry so a person can put a control
                        // back to the platform's own default, which is not the
                        // same as any value it offers.
                        <option value="" selected=value.is_empty()>{format!("{label}: any")}</option>
                        {offered.into_iter().map(|choice| {
                            let selected = choice.value == value;
                            let shown = choice.label.clone();
                            view! {
                                <option value=choice.value.clone() selected=selected>{shown}</option>
                            }
                        }).collect_view()}
                    </select>
                }.into_any()
            }).collect_view()
            }

            <button
                class="drop"
                title="Remove from this surface"
                on:click=move |_| on_remove(id.clone())
            >"×"</button>
        </header>
    }
    .into_any()
}

/// A panel's contents, drawn through the design pack.
#[component]
fn PanelBody(
    drawer: Drawer,
    known: Option<crate::state::PanelView>,
    abandoned: bool,
    kind_override: Option<String>,
    catalogued: Option<CatalogPanel>,
    /// What this panel's components may change, and what it is set to.
    controls: Vec<hlin_view::Control>,
    /// Where they say a person changed it.
    emit: hlin_view::Emit,
) -> impl IntoView {
    let Some(panel) = known else {
        // On the surface, not yet in the stream. Either the layout has just
        // been written and the first frames are on their way, which is
        // `loading`, or the stream stopped answering long enough ago that
        // nothing is coming, which is not.
        let state = if abandoned {
            hlin_view::PanelState::Unavailable(hlin_view::Cause::Unreachable)
        } else {
            hlin_view::PanelState::Loading
        };

        let drawn = plan(state, Kind::Raw, None);
        let body = drawer.draw(&drawn, None);

        return view! {
            {body}
            {abandoned.then(|| view! {
                <p class="detail">"Hlin is not responding"</p>
            })}
        }
        .into_any();
    };

    // The viewer's choice, then the platform's default, then whatever the
    // vocabulary prefers for this envelope. An unknown name lands on `raw`, so
    // a panel always draws.
    let kind = kind_override
        .as_deref()
        .or(catalogued.as_ref().map(|entry| entry.kind.as_str()))
        .map(Kind::resolve)
        .unwrap_or_else(|| preferred_kind(&panel));

    // A component the platform asked for, forwarded to the pack untouched —
    // unless the viewer has picked a kind, in which case they have said what
    // they want this panel to look like and that outranks the platform's
    // suggestion. Switching a panel to a table and getting a graph anyway would
    // make the kind picker a lie.
    let component = kind_override
        .is_none()
        .then(|| {
            catalogued
                .as_ref()
                .and_then(|entry| entry.component.clone())
        })
        .flatten();

    // A component the mounted pack does not list is said once, here, because
    // the alternative is silence: a platform that typed `aurora.grpah` gets its
    // declared kind and looks exactly like a shell running a pack without that
    // component. The panel still draws — this only says so.
    if let Some(name) = component.as_deref()
        && !drawer.offers().contains(&name)
    {
        leptos::logging::warn!(
            "this front end's design pack does not offer `{name}`; \
             drawing the panel's declared kind instead"
        );
    }

    let drawn = plan(panel.state, kind, panel.envelope.as_ref())
        .drawn_by(component.as_deref())
        .interactive(controls, emit);
    let body = drawer.draw(&drawn, panel.age_seconds);

    view! {
        {body}
        {panel.detail.clone().map(|text| view! { <p class="detail">{text}</p> })}
    }
    .into_any()
}

/// Two date boxes and a button, for a range no preset covers.
#[component]
fn CustomRange(on_apply: impl Fn(TimeRange) + 'static) -> impl IntoView {
    let (from, set_from) = signal(String::new());
    let (to, set_to) = signal(String::new());

    view! {
        <span class="custom">
            <input
                type="datetime-local"
                prop:value=move || from.get()
                on:change=move |event| set_from.set(event_value(&event))
            />
            <input
                type="datetime-local"
                prop:value=move || to.get()
                on:change=move |event| set_to.set(event_value(&event))
            />
            <button
                disabled=move || parse_range(&from.get(), &to.get()).is_none()
                on:click=move |_| {
                    if let Some(range) = parse_range(&from.get(), &to.get()) {
                        on_apply(range);
                    }
                }
            >"Apply"</button>
        </span>
    }
}

/// What a pair of `datetime-local` boxes means, if it means anything.
///
/// A browser writes these without a zone, so they are read as local time and
/// converted, because everything past this point is absolute
/// ([[HLIN-S-0002]]). A range that runs backwards is not a range, and the
/// button stays disabled rather than sending the shell something it would have
/// to interpret.
fn parse_range(from: &str, to: &str) -> Option<TimeRange> {
    let from = local(from)?;
    let to = local(to)?;
    (from < to).then_some(TimeRange { from, to })
}

fn local(value: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        .map(|local| local.with_timezone(&Utc))
}

/// The layout named by the address, where the address names one.
///
/// `/s/{id}` is the surface route; anything else, including `/`, means "give me
/// whichever surface is mine".
fn requested_layout() -> Option<String> {
    let path = web_sys::window()?.location().pathname().ok()?;
    let id = path.strip_prefix("/s/")?.trim_end_matches('/');
    (!id.is_empty()).then(|| id.to_string())
}

/// Put the surface's own address in the address bar, without navigating.
///
/// A failure here is deliberately ignored: the surface works whatever the
/// address says, and a browser refusing a history entry is not worth a message
/// to a person who did not ask for one.
fn show_address(path: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

/// Whether a panel matches what is typed in the picker's search box.
fn matches(panel: &CatalogPanel, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    panel.title.to_lowercase().contains(needle)
        || panel.key.to_lowercase().contains(needle)
        || panel
            .description
            .as_deref()
            .is_some_and(|text| text.to_lowercase().contains(needle))
}

/// The catalogue entry for an instance, where its platform still declares it.
fn catalogued(
    catalog: &[CatalogPlatform],
    platform_id: &str,
    panel_key: &str,
) -> Option<CatalogPanel> {
    catalog
        .iter()
        .find(|platform| platform.id == platform_id)?
        .panels
        .iter()
        .find(|panel| panel.key == panel_key)
        .cloned()
}

/// What to call a panel: the viewer's title, then the platform's, then the key.
fn title_of(
    instance: &hlin_stream::layout::PanelInstanceDocument,
    catalogued: Option<&CatalogPanel>,
) -> String {
    instance
        .title_override
        .clone()
        .or_else(|| catalogued.map(|entry| entry.title.clone()))
        .unwrap_or_else(|| readable(&instance.panel_key))
}

/// The kind to draw a panel with when nobody has said.
fn preferred_kind(panel: &crate::state::PanelView) -> Kind {
    let Some(envelope) = &panel.envelope else {
        return Kind::Raw;
    };
    Kind::accepting(envelope.name())
        .into_iter()
        .find(|kind| *kind != Kind::Raw)
        .unwrap_or(Kind::Raw)
}

/// A panel's state, as one word for the markup.
///
/// The same vocabulary the stream uses, so what a test reads off the page is
/// what the shell said rather than a second naming of the same four states.
fn state_name(state: hlin_view::PanelState) -> &'static str {
    match state {
        hlin_view::PanelState::Loading => "loading",
        hlin_view::PanelState::Ready => "ready",
        hlin_view::PanelState::Stale => "stale",
        hlin_view::PanelState::Unavailable(_) => "unavailable",
    }
}

/// A panel key, as something a person can read.
fn readable(key: &str) -> String {
    key.split('-')
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What was typed into an input or chosen in a select.
fn event_value(event: &leptos::ev::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .or_else(|| {
            event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
                .map(|select| select.value())
        })
        .unwrap_or_default()
}

/// A time range from the milliseconds a component reported.
///
/// Milliseconds because that is what `series.v1` points carry, so a component
/// reading a brush off its own axis has nothing to convert. Refused rather than
/// corrected where the two ends do not make a range: a component with a bug in
/// its brush should leave the surface where it was, not send everything to an
/// interval nobody asked for.
fn span(from_millis: i64, to_millis: i64) -> Option<TimeRange> {
    if from_millis >= to_millis {
        return None;
    }
    Some(TimeRange {
        from: DateTime::from_timestamp_millis(from_millis)?,
        to: DateTime::from_timestamp_millis(to_millis)?,
    })
}
