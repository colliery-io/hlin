//! The surface a person looks at, and composes.
//!
//! Everything with a decision in it lives elsewhere: `state` holds what the
//! browser believes about the panels, `draft` holds the layout being edited,
//! `grid` holds where things sit. This module is the wiring between those and
//! the DOM, and it is deliberately thin, because none of it can be tested
//! without a browser.

use std::collections::{BTreeMap, BTreeSet};

use crate::pack::Drawer;
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use hlin_manifest::envelope::Choice;
use hlin_stream::layout::{
    CatalogPanel, CatalogPlatform, LayoutDocument, LayoutItem, Page, Placement,
};
use hlin_stream::{Frame, ParamsRequest, TimeRange};
use hlin_view::{Kind, plan};
use leptos::ev::PointerEvent;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::content::Opened;
use crate::draft::LayoutDraft;
use crate::frame::{ModuleFrame, ModuleView, Mount};
use crate::grid;
use crate::route::{self, PageTarget};
use crate::state::{PanelView, SurfaceState};

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
/// Views a front end writes in code, by id (decision HLIN-A-0015).
///
/// A content component opens one when its page is `{ "view": "<id>" }`. An id
/// the front end does not register shows a placeholder saying so; the page is
/// not dropped from the layout that names it.
///
/// ```ignore
/// let views = Views::new().with("about", || view! { <p>"About this shell"</p> });
/// view! { <App pack=MyPack views=views /> }
/// ```
#[derive(Clone, Default)]
pub struct Views(BTreeMap<String, ViewFn>);

impl Views {
    /// No views.
    pub fn new() -> Self {
        Self::default()
    }

    /// These views and one more, under `id`.
    pub fn with(mut self, id: impl Into<String>, view: impl Into<ViewFn>) -> Self {
        self.0.insert(id.into(), view.into());
        self
    }

    /// The ids registered.
    pub fn ids(&self) -> BTreeSet<String> {
        self.0.keys().cloned().collect()
    }

    fn get(&self, id: &str) -> Option<ViewFn> {
        self.0.get(id).cloned()
    }
}

/// A surface on screen, as the shell-wide handlers need to know it: which
/// panels are on it and where a module's intent for one of them goes.
#[derive(Clone)]
struct Owner {
    depth: usize,
    draft: ReadSignal<LayoutDraft>,
    act: Callback<(String, hlin_view::Intent)>,
}

/// What the shell's components share across every surface on screen: who is
/// looking, what there is, the one time range, and the full-width page.
///
/// Provided by [`App`] as a context. Each surface adds its own beside it, so
/// [`Bar`] and [`SurfaceView`] read the surface they are drawn in. A component
/// drawn outside an `App` has no shell to read, and panics saying so.
#[derive(Clone, Copy)]
pub struct Shell {
    catalog: ReadSignal<Vec<CatalogPlatform>>,
    catalog_known: ReadSignal<bool>,
    choices: ReadSignal<BTreeMap<(String, String), Vec<Choice>>>,
    set_choices: WriteSignal<BTreeMap<(String, String), Vec<Choice>>>,
    mode: ReadSignal<Mode>,
    set_mode: WriteSignal<Mode>,
    connected: ReadSignal<bool>,
    set_connected: WriteSignal<bool>,
    grace: ReadSignal<i64>,
    read_only: ReadSignal<bool>,
    viewer: ReadSignal<Option<String>>,
    sign_out: ReadSignal<Option<String>>,
    brand: ReadSignal<Option<hlin_stream::layout::BrandSummary>>,
    trouble: ReadSignal<Option<String>>,
    set_trouble: WriteSignal<Option<String>>,
    chosen_range: ReadSignal<i64>,
    set_chosen_range: WriteSignal<i64>,
    custom: ReadSignal<Option<TimeRange>>,
    set_custom: WriteSignal<Option<TimeRange>>,
    asked_range: ReadSignal<Option<(TimeRange, u64)>>,
    modules: RwSignal<BTreeMap<String, ModuleView>>,
    open: Memo<Option<PageTarget>>,
    wants_time: Memo<bool>,
    views: StoredValue<Views>,
    /// The outermost surface's id, once it is known: where "back" goes.
    top: RwSignal<Option<String>>,
    /// Each surface on screen, by a number of the page's own.
    owners: StoredValue<BTreeMap<u64, Owner>>,
    serial: StoredValue<u64>,
    /// What each surface's modules are told about their panels, by surface.
    contexts: RwSignal<BTreeMap<u64, BTreeMap<String, crate::frame::PanelContext>>>,
    /// Whether anything on each surface responds to the time range.
    wanting: RwSignal<BTreeMap<u64, bool>>,
    /// The last `changed` handed to the frames, and when, so the same news on
    /// two surfaces' streams is passed on once.
    last_changed: StoredValue<Option<(String, f64)>>,
    reapply: Callback<()>,
    apply_preset: Callback<i64>,
    apply_range: Callback<TimeRange>,
    open_page: Callback<PageTarget>,
    close_page: Callback<()>,
}

impl Shell {
    fn next_key(&self) -> u64 {
        let mut key = 0;
        self.serial.update_value(|serial| {
            *serial += 1;
            key = *serial;
        });
        key
    }

    /// The surface a panel instance is on, or the outermost one.
    fn owner_of(&self, instance: &str) -> Option<Owner> {
        self.owners.with_value(|owners| {
            owners
                .values()
                .find(|owner| {
                    owner
                        .draft
                        .with_untracked(|draft| draft.panel(instance).is_some())
                })
                .or_else(|| owners.values().min_by_key(|owner| owner.depth))
                .cloned()
        })
    }

    /// Hand a `changed` frame to the frames, unless the same one was handed on
    /// a moment ago from another surface's stream.
    fn changed(&self, changed: hlin_stream::ChangedFrame) {
        let key = serde_json::to_string(&changed).unwrap_or_default();
        let now = js_sys::Date::now();
        let repeat = self.last_changed.with_value(|last| {
            last.as_ref()
                .is_some_and(|(seen, at)| *seen == key && now - at < CHANGED_ECHO_MS)
        });
        self.last_changed.set_value(Some((key, now)));
        if !repeat {
            crate::frame::heard(changed);
        }
    }
}

/// How long the same `changed` arriving on a second surface's stream counts as
/// the same news.
const CHANGED_ECHO_MS: f64 = 500.0;

fn shell() -> Shell {
    use_context::<Shell>()
        .expect("a shell component is drawn inside `App`, which provides the shell")
}

/// One surface on screen: its layout, its stream, and what may be done to it.
///
/// Provided by each [`SurfaceHost`] for what it draws, so a surface opened in
/// a content component has its own, and the components inside it read that one.
#[derive(Clone, Copy)]
struct SurfaceScope {
    depth: usize,
    /// The ids of every surface around this one, and its own once known.
    ancestry: StoredValue<Vec<String>>,
    state: ReadSignal<SurfaceState>,
    draft: ReadSignal<LayoutDraft>,
    set_draft: WriteSignal<LayoutDraft>,
    writing: ReadSignal<u32>,
    save: Callback<()>,
    act: Callback<(String, hlin_view::Intent)>,
    /// The page each content component shows, by its id. A page selector
    /// names one of these as its target.
    contents: StoredValue<BTreeMap<String, RwSignal<Option<Page>>>>,
}

fn scope() -> SurfaceScope {
    use_context::<SurfaceScope>()
        .expect("a surface component is drawn inside a surface, which provides its scope")
}

/// Which surface a [`SurfaceHost`] shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// The one the address names, or the person's own.
    Top,
    /// A surface opened in a content component.
    Surface(String),
}

/// The whole thing.
///
/// Generic over the design pack, which is the only thing a consumer has to
/// supply. The pack is erased into a [`Drawer`] on the first line, so nothing
/// below this component is generic and nothing below it knows which design
/// system is drawing.
#[component]
pub fn App<P>(
    /// What draws the panels. The only thing a consumer has to supply.
    pack: P,
    /// The front end's own controls, at the end of the bar: whatever its
    /// design system offers a person beyond the panels, such as a theme
    /// switch. Optional, and nothing in this crate knows what it holds.
    #[prop(optional, into)]
    bar: Option<ViewFn>,
    /// Views the front end writes in code, which a content component opens
    /// by id.
    #[prop(optional)]
    views: Views,
) -> impl IntoView
where
    P: hlin_view::DesignPack + Send + Sync + 'static,
    P::View: IntoView + 'static,
{
    let drawer = Drawer::of(pack);

    let (catalog, set_catalog) = signal(Vec::<CatalogPlatform>::new());
    // Whether the catalogue has been asked for yet, so a page nobody offers
    // is told apart from one whose platform the browser has not heard of yet.
    let (catalog_known, set_catalog_known) = signal(false);
    // The platform page open at full width, if one is (HLIN-S-0007, *Pages*).
    // Read from the address first, so a link to a page opens that page.
    let (page, set_page) = signal(route::page_of(&current_path()));
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
    // Who the shell says this is, and where to end their session if it keeps
    // one. Both empty until `/api/config` answers.
    let (viewer, set_viewer) = signal(Option::<String>::None);
    let (sign_out, set_sign_out) = signal(Option::<String>::None);
    // Whose name and logo the bar shows. Nothing until the shell says, so a
    // shell wearing another name never shows Hlin's first.
    let (brand, set_brand) = signal(Option::<hlin_stream::layout::BrandSummary>::None);
    let (trouble, set_trouble) = signal(Option::<String>::None);
    let (chosen_range, set_chosen_range) = signal(3600i64);
    let (custom, set_custom) = signal(Option::<TimeRange>::None);

    // What each module panel's bridge says about it, by instance. Written by
    // `frame`, which owns the frames; read here to draw the panels around them.
    let modules = RwSignal::new(BTreeMap::<String, ModuleView>::new());
    crate::frame::start(modules);

    // What the shell says about itself, and what there is to compose from.
    // The surfaces fetch their own layouts.
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(config) = crate::api::config().await {
                set_grace.set(config.stream_loss_grace_seconds);
                set_read_only.set(config.read_only);
                // The name where there is one, the identifier otherwise: a
                // person should see who the shell thinks they are either way.
                let shown = config.principal.name.unwrap_or(config.principal.sub);
                // Every module is told the same name, for display only: a
                // platform learns who is asking from the token on each request.
                crate::frame::set_viewer(Some(shown.clone()), config.read_only);
                set_viewer.set(Some(shown));
                set_sign_out.set(config.sign_out);
                // Whose page this is. The title too, which the front end's
                // `index.html` leaves empty so no other name shows first.
                crate::brand::set_name(&config.brand.name);
                if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                    document.set_title(&config.brand.name);
                }
                set_brand.set(Some(config.brand));
            }
            if let Ok(platforms) = crate::api::catalog().await {
                // Options are fetched by each surface, for the panels it
                // holds, because which ones are wanted changes while a person
                // composes.
                set_catalog.set(platforms);
            }
            set_catalog_known.set(true);
        });
    });

    // -- The time picker --------------------------------------------------

    // The range the shell was last asked for, and the generation that asked.
    // One for the whole page: every surface on screen follows it (D19), and
    // modules are told it as `context`, so a module and the panels beside it
    // answer the same question.
    let (asked_range, set_asked_range) = signal(Option::<(TimeRange, u64)>::None);
    let generation = StoredValue::new(0u64);

    let apply_range = move |range: TimeRange| {
        let mut next = 0;
        generation.update_value(|counted| {
            *counted += 1;
            next = *counted;
        });
        set_asked_range.set(Some((range, next)));
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
    // per-panel selection changes, and once when the first surface opens, so
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

    // -- What modules are told --------------------------------------------

    let contexts =
        RwSignal::new(BTreeMap::<u64, BTreeMap<String, crate::frame::PanelContext>>::new());
    let wanting = RwSignal::new(BTreeMap::<u64, bool>::new());
    let owners = StoredValue::new(BTreeMap::<u64, Owner>::new());

    // The range the shell was last asked for, and every panel's own declared
    // parameters, on every surface. The frame host diffs it per module, so
    // this may run on every change to a draft — a drag included — and a module
    // hears only what moved for it.
    Effect::new(move |_| {
        let Some((range, generation)) = asked_range.get() else {
            return;
        };
        let panels = contexts.with(|contexts| {
            contexts
                .values()
                .flat_map(|panels| panels.clone())
                .collect::<BTreeMap<_, _>>()
        });
        crate::frame::set_context(
            Some(hlin_bridge::TimeRange {
                from_millis: range.from.timestamp_millis(),
                to_millis: range.to.timestamp_millis(),
            }),
            generation,
            panels,
        );
    });

    // -- Pages -------------------------------------------------------------

    let top = RwSignal::new(Option::<String>::None);

    // Open a platform's page, and say so in the address bar, as a new entry in
    // the history: a page is somewhere a person went, and the browser's back
    // button should bring them back from it.
    let open_page = move |target: PageTarget| {
        if page.with_untracked(|open| open.as_ref() == Some(&target)) {
            return;
        }
        push_address(&route::page_address(&target));
        set_mode.set(Mode::Watching);
        set_page.set(Some(target));
    };

    // Back to the surface, which was only ever hidden.
    let close_page = move || {
        if page.with_untracked(Option::is_none) {
            return;
        }
        let surface = top.get_untracked();
        push_address(&surface.map_or_else(|| "/".to_string(), |id| format!("/s/{id}")));
        set_page.set(None);
    };

    // The browser's back and forward move between the surface and pages, and
    // the address is the only record of which.
    Effect::new(move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };
        let popped = Closure::<dyn Fn()>::new(move || {
            set_page.set(route::page_of(&current_path()));
        });
        let _ =
            window.add_event_listener_with_callback("popstate", popped.as_ref().unchecked_ref());
        popped.forget();
    });

    // Only a change of page redraws one: setting the same page again must not
    // reload its module.
    let open = Memo::new(move |_| page.get());

    // Whether the bar carries the time controls (HLIN-T-0078): only while the
    // surfaces are on screen and something on one of them declares
    // `time_range`. Absent rather than disabled otherwise, for the same reason
    // Edit is on a read-only shell. The range itself is kept while they are
    // gone — the picker's signals are not touched, and modules are still told
    // it — so a time-driven panel added back resumes the range that was in
    // force.
    //
    // A page counts as wanting none. A navigation entry declares nothing about
    // time, and the module it opens is told the surface's range as `context`
    // either way; controls the page cannot be shown to use are the same noise
    // on a page as on a surface of lists.
    let wants_time = Memo::new(move |_| {
        open.with(Option::is_none) && wanting.with(|wanting| wanting.values().any(|wants| *wants))
    });

    let shell = Shell {
        catalog,
        catalog_known,
        choices,
        set_choices,
        mode,
        set_mode,
        connected,
        set_connected,
        grace,
        read_only,
        viewer,
        sign_out,
        brand,
        trouble,
        set_trouble,
        chosen_range,
        set_chosen_range,
        custom,
        set_custom,
        asked_range,
        modules,
        open,
        wants_time,
        views: StoredValue::new(views),
        top,
        owners,
        serial: StoredValue::new(0),
        contexts,
        wanting,
        last_changed: StoredValue::new(None),
        reapply: Callback::new(move |()| reapply()),
        apply_preset: Callback::new(apply_preset),
        apply_range: Callback::new(apply_range),
        open_page: Callback::new(open_page),
        close_page: Callback::new(move |()| close_page()),
    };
    provide_context(shell);

    // What a module asks of the shell, sent to the surface its panel is on.
    crate::frame::on_asked(move |asked| match asked {
        crate::frame::Asked::Intent { instance, intent } => {
            if let Some(owner) = shell.owner_of(&instance) {
                owner.act.run((instance, intent));
            }
        }
        crate::frame::Asked::Navigate { instance, to } => {
            if let Some(path) = &to.page {
                let target = PageTarget {
                    platform: to.platform.clone(),
                    path: path.clone(),
                };
                if catalog.with_untracked(|catalog| route::page_entry(catalog, &target).is_some()) {
                    open_page(target);
                } else {
                    leptos::logging::warn!(
                        "a module asked to open page `{path}` of `{}`, which is not a page this shell can open; ignored",
                        to.platform
                    );
                }
                return;
            }
            let Some(owner) = shell.owner_of(&instance) else {
                return;
            };
            let Some(target) = panel_to_open(
                &owner.draft.get_untracked(),
                &catalog.get_untracked(),
                &instance,
                &to,
            ) else {
                return;
            };
            if page.with_untracked(Option::is_none) {
                bring_into_view(&target);
                return;
            }
            // Asked from a page: back to the surface, and to the panel once it
            // is drawn again.
            close_page();
            leptos::task::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(0).await;
                bring_into_view(&target);
            });
        }
    });

    let styling = drawer.stylesheet();
    let page_drawer = drawer.clone();

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

        // The operator's, after the pack's so it wins: it may set the
        // chrome's roles, the pack's own tokens, or anything else. Always
        // linked, because the shell answers an empty sheet where there is no
        // brand. When it arrives the page's colours may have moved, so every
        // module is told them again.
        <link
            rel="stylesheet"
            href=crate::brand::STYLESHEET_PATH
            on:load=move |_| crate::frame::retheme()
        />

        <SurfaceHost source=Source::Top depth=0 ancestry=Vec::new() drawer=drawer chrome=bar />

        {move || open.get().map(|target| view! {
            <PageView
                drawer=page_drawer.clone()
                target=target
                catalog=catalog
                catalog_known=catalog_known
                modules=modules
            />
        })}
    }
}

/// One surface on screen, from fetching its layout to drawing it.
///
/// The outermost also draws the bar, the page links and any trouble above it,
/// as the shell's chrome. A surface opened in a content component draws only
/// itself, one level deeper, and says so in its own cells if it cannot be
/// fetched.
#[component]
fn SurfaceHost(
    source: Source,
    depth: usize,
    /// The ids of every surface around this one.
    ancestry: Vec<String>,
    drawer: Drawer,
    /// The front end's own controls for the outermost bar.
    #[prop(default = None)]
    chrome: Option<ViewFn>,
) -> AnyView {
    let shell = shell();
    let key = shell.next_key();
    let top = source == Source::Top;

    let (state, set_state) = signal(SurfaceState::new());
    let (draft, set_draft) = signal(LayoutDraft::empty());
    // Why this surface could not be fetched, where it is not the outermost:
    // the outermost says so in the shell's own trouble line.
    let (missing, set_missing) = signal(Option::<String>::None);

    let ancestry = StoredValue::new({
        let mut around = ancestry;
        if let Source::Surface(id) = &source {
            around.push(id.clone());
        }
        around
    });

    // Bumped whenever the surface must be re-subscribed: when the layout is
    // first known. A write does not bump it; the shell reconciles a running
    // surface against what was written.
    let (revision, set_revision) = signal(0u32);

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

    // Gone from the screen: its loops stop and its stream closes.
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

    // The layout. A link to a surface names it; a bare visit gets the
    // principal's own. Either way the address bar ends up naming what is on
    // screen, so the link can be sent to somebody. A link to a page is what is
    // on screen, and keeps its address; the surface behind it is the
    // principal's own.
    Effect::new({
        let source = source.clone();
        move |_| {
            let source = source.clone();
            leptos::task::spawn_local(async move {
                let asked = match &source {
                    Source::Top => requested_layout(),
                    Source::Surface(id) => Some(id.clone()),
                };
                let fetched = match &asked {
                    Some(id) => crate::api::layout(id).await,
                    None => crate::api::home().await,
                };
                match fetched {
                    Ok(document) => {
                        if source == Source::Top
                            && let Some(id) = document.surface_id()
                        {
                            if asked.is_none() && shell.open.get_untracked().is_none() {
                                show_address(&format!("/s/{id}"));
                            }
                            shell.top.set(Some(id.to_string()));
                            ancestry.update_value(|around| around.push(id.to_string()));
                        }
                        set_draft.set(LayoutDraft::of(document));
                        set_revision.update(|revision| *revision += 1);
                    }
                    Err(reason) if source == Source::Top => shell.set_trouble.set(Some(reason)),
                    Err(reason) => set_missing.set(Some(reason)),
                }
            });
        }
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
        let platforms = shell.catalog.get();
        if wanted_for.is_empty() || platforms.is_empty() {
            return;
        }

        // Only what is not already known. A person adding a second panel from a
        // platform should not re-ask for the choices the first one fetched.
        let known = shell.choices.with_untracked(|listed| {
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

            shell.set_choices.update(|listed| listed.extend(fetched));
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
        // layout fetched, which is exactly when the surface's panels can have
        // changed.
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

        // A module's `changed` is relayed through the outermost surface's
        // stream; the shell passes it to every surface it serves.
        if depth == 0 {
            crate::frame::set_surface(Some(surface_id.clone()));
        }

        let applied = move |frame: Frame| {
            // News for the modules, not a panel's state: handed to the frames,
            // and the surface is not redrawn for it.
            if let Frame::Changed(changed) = frame {
                shell.changed(changed);
                return;
            }
            set_state.update(|state| {
                state.apply(&frame);
            });
        };
        let lost = move || {
            if depth == 0 {
                shell.set_connected.set(false);
            }
            set_state.update(|state| state.on_stream_lost(Utc::now()));
        };
        let opened = move || {
            if depth == 0 {
                shell.set_connected.set(true);
            }
            set_state.update(|state| state.on_stream_restored());
        };

        if let Ok(source) =
            crate::stream::open(&format!("/api/stream/{surface_id}"), applied, lost, opened)
        {
            held.set_value(Some(source));
        }
    });

    // While the stream is gone, the browser decides for itself when "not
    // current" becomes "not coming".
    Effect::new({
        let alive = alive.clone();
        move |_| {
            let alive = alive.clone();
            leptos::task::spawn_local(async move {
                while alive.load(std::sync::atomic::Ordering::Relaxed) {
                    gloo_timers::future::TimeoutFuture::new(1_000).await;
                    let grace = shell.grace.get_untracked();
                    set_state.update(|state| {
                        state.on_tick(Utc::now(), Duration::seconds(grace));
                    });
                }
            });
        }
    });

    // The range in force, sent to this surface's stream: once the layout is
    // known, and again whenever the range or a selection moves it.
    Effect::new(move |_| {
        let revision = revision.get();
        let asked = shell.asked_range.get();
        if revision == 0 {
            return;
        }
        let Some((range, generation)) = asked else {
            // The first surface to open asks for the range the picker shows.
            shell.reapply.run(());
            return;
        };
        set_state.update(|state| state.adopt_generation(generation));
        let request = ParamsRequest {
            protocol_version: hlin_stream::PROTOCOL_VERSION,
            generation,
            time_range: Some(range),
            selections: draft.with_untracked(|draft| draft.selections()),
        };
        let surface_id = draft.with_untracked(|draft| draft.surface_id().map(str::to_string));
        leptos::task::spawn_local(async move {
            if let Some(surface_id) = surface_id {
                let _ = crate::stream::set_params(&surface_id, &request).await;
            }
        });
    });

    // What this surface's modules are told about their own panels, and whether
    // it wants the time controls.
    Effect::new(move |_| {
        let panels = draft.with(|draft| {
            shell.catalog.with(|catalog| {
                draft
                    .panels()
                    .iter()
                    .filter_map(|panel| {
                        let declared = catalogued(catalog, &panel.platform_id, &panel.panel_key)
                            .map(|entry| {
                                entry
                                    .controls
                                    .into_iter()
                                    .map(|control| control.id)
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some((
                            panel.id.clone()?,
                            crate::frame::PanelContext {
                                selections: panel.selections.clone(),
                                declared,
                            },
                        ))
                    })
                    .collect()
            })
        });
        shell.contexts.update(|contexts| {
            contexts.insert(key, panels);
        });
        let wants = draft.with(|draft| shell.catalog.with(|catalog| draft.wants_time(catalog)));
        if shell
            .wanting
            .with_untracked(|wanting| wanting.get(&key) != Some(&wants))
        {
            shell.wanting.update(|wanting| {
                wanting.insert(key, wants);
            });
        }
    });

    // -- Writing ---------------------------------------------------------

    // One write, carrying the whole layout, issued when a gesture ends rather
    // than while it is in progress.
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
                    shell.set_trouble.set(None);

                    // Deliberately no `set_revision` here. The shell reconciles
                    // a running surface against what was written rather than
                    // dropping it, so the stream already carries the new
                    // arrangement — and re-subscribing would throw away every
                    // panel's data to fetch the same values again.
                }
                Err(reason) => shell.set_trouble.set(Some(reason)),
            }
        });
    };

    // What a person does to a panel, whoever drew the thing they did it to:
    // the chrome's own control, a component a pack drew, or the platform's
    // module in its frame. One handler, so all three have exactly the same
    // effect — stored with the layout, sent to the shell, and back to every
    // module as `context`.
    let act = move |who: String, intent: hlin_view::Intent| match intent {
        hlin_view::Intent::Select { param, values } => {
            if who.is_empty() {
                return;
            }
            set_draft.update(|draft| draft.set_selection(&who, &param, values));
            save();
            shell.reapply.run(());
        }
        hlin_view::Intent::Range {
            from_millis,
            to_millis,
        } => {
            let Some(range) = span(from_millis, to_millis) else {
                return;
            };
            // The picker is told as well as the shell. A chart brushed to an
            // hour that left the bar still saying "24h" would be the chrome
            // lying about what is on screen.
            shell.set_custom.set(Some(range));
            shell.set_chosen_range.set(0);
            shell.apply_range.run(range);
        }
    };
    let act = Callback::new(move |(who, intent)| act(who, intent));

    shell.owners.update_value(|owners| {
        owners.insert(key, Owner { depth, draft, act });
    });
    on_cleanup({
        let alive = alive.clone();
        move || {
            alive.store(false, std::sync::atomic::Ordering::Relaxed);
            let _ = held.try_update_value(|existing| {
                if let Some(source) = existing.take() {
                    source.close();
                }
            });
            let _ = shell.owners.try_update_value(|owners| {
                owners.remove(&key);
            });
            let _ = shell.contexts.try_update(|contexts| {
                contexts.remove(&key);
            });
            let _ = shell.wanting.try_update(|wanting| {
                wanting.remove(&key);
            });
        }
    });

    provide_context(SurfaceScope {
        depth,
        ancestry,
        state,
        draft,
        set_draft,
        writing,
        save: Callback::new(move |()| save()),
        act,
        contents: StoredValue::new(BTreeMap::new()),
    });

    if !top {
        return view! {
            {move || match missing.get() {
                Some(reason) => view! {
                    <p class="content-missing" data-cause="unreachable">{reason}</p>
                }
                .into_any(),
                None => view! { <SurfaceView drawer=drawer.clone() /> }.into_any(),
            }}
        }
        .into_any();
    }

    view! {
        {match chrome {
            Some(extra) => view! { <Bar extra=extra /> }.into_any(),
            None => view! { <Bar /> }.into_any(),
        }}

        <PageLinks />

        {move || shell.trouble.get().map(|reason| view! {
            <p class="trouble">{reason}</p>
        })}

        <SurfaceView drawer=drawer />
    }
    .into_any()
}

/// The platforms' pages (HLIN-S-0007, *Pages*), and the way back to the
/// surface, whenever there is a page to open or one is open. Only a navigation
/// entry with a module is a page; the others were never drawn by the shell and
/// are not now.
#[component]
fn PageLinks() -> impl IntoView {
    let Shell {
        catalog,
        open,
        open_page,
        close_page,
        ..
    } = shell();
    let SurfaceScope { draft, .. } = scope();

    move || {
        let offered = catalog.with(|catalog| route::pages(catalog));
        if offered.is_empty() && open.with(Option::is_none) {
            return None;
        }
        let surface = draft
            .with(|draft| draft.surface_id().map(|id| format!("/s/{id}")))
            .unwrap_or_else(|| "/".to_string());
        let mut previous: Option<String> = None;
        let links = offered
            .into_iter()
            .map(|entry| {
                // Each platform's name once, before its first page.
                let heading = (previous.as_deref() != Some(entry.platform.as_str())).then(|| {
                    previous = Some(entry.platform.clone());
                    view! { <span class="pages-platform">{entry.platform_name.clone()}</span> }
                });
                let target = entry.target();
                let href = route::page_address(&target);
                let current = {
                    let target = target.clone();
                    move || {
                        open.with(|open| open.as_ref() == Some(&target))
                            .then_some("page")
                    }
                };
                view! {
                    {heading}
                    <a
                        class="page-link"
                        href=href
                        data-page=format!("{}/{}", entry.platform, entry.path)
                        title=format!("{} — {}", entry.label, entry.platform_name)
                        aria-current=current
                        on:click=move |event: leptos::ev::MouseEvent| {
                            if plain_click(&event) {
                                event.prevent_default();
                                open_page.run(target.clone());
                            }
                        }
                    >
                        {entry.label.clone()}
                    </a>
                }
            })
            .collect_view();
        Some(view! {
            <nav class="pages" aria-label="Pages">
                <a
                    class="page-link surface-link"
                    href=surface
                    aria-current=move || open.with(Option::is_none).then_some("page")
                    on:click=move |event: leptos::ev::MouseEvent| {
                        if plain_click(&event) {
                            event.prevent_default();
                            close_page.run(());
                        }
                    }
                >
                    "Surface"
                </a>
                {links}
            </nav>
        })
    }
}

/// A content component: one page, in its cells (decision HLIN-A-0015, D9).
///
/// It shows its `initial` page until a page selector chooses another. What it
/// may open is decided by [`crate::content::open`]; anything it cannot open is
/// a placeholder saying why.
#[component]
fn Content(
    /// Its id on the surface, which a page selector names as its target.
    id: String,
    /// The page it shows first.
    initial: Option<Page>,
    drawer: Drawer,
) -> AnyView {
    let shell = shell();
    let scope = scope();

    let shown = RwSignal::new(initial);
    scope.contents.update_value(|contents| {
        contents.insert(id, shown);
    });

    let views = shell.views;
    let opened = Memo::new(move |_| {
        let page = shown.get();
        let ancestry = scope.ancestry.get_value();
        let known = shell.catalog_known.get();
        let ids = views.with_value(Views::ids);
        shell.catalog.with(|catalog| {
            crate::content::open(page.as_ref(), scope.depth, &ancestry, catalog, known, &ids)
        })
    });

    (move || match opened.get() {
        Opened::Surface(surface) => view! {
            <SurfaceHost
                source=Source::Surface(surface)
                depth=scope.depth + 1
                ancestry=scope.ancestry.get_value()
                drawer=drawer.clone()
            />
        }
        .into_any(),
        Opened::Platform(target) => view! {
            <PageView
                drawer=drawer.clone()
                target=target
                catalog=shell.catalog
                catalog_known=shell.catalog_known
                modules=shell.modules
            />
        }
        .into_any(),
        Opened::View(view) => views
            .with_value(|views| views.get(&view))
            .map(|drawn| drawn.run())
            .unwrap_or_else(|| ().into_any()),
        Opened::Waiting => ().into_any(),
        Opened::Missing(missing) => view! {
            <p class="content-missing" data-cause=missing.cause()>{missing.to_string()}</p>
        }
        .into_any(),
    })
    .into_any()
}

/// The bar across the top: whose shell this is, the time controls, whether the
/// surface is live and saved, Edit, who is signed in, and the front end's own
/// controls at the end.
///
/// Drawn inside [`App`], which provides the [`Shell`] it reads.
#[component]
pub fn Bar(
    /// The front end's own controls, at the end of the bar: whatever its
    /// design system offers a person beyond the panels, such as a theme
    /// switch. Optional, and nothing in this crate knows what it holds.
    #[prop(optional, into)]
    extra: Option<ViewFn>,
    /// A bar placed on a surface rather than the shell's own: drawn without
    /// the time picker, because the outermost picker sets the range for every
    /// surface on screen (D19, D21), and without measuring itself for the
    /// catalogue, which sticks below the shell's bar only.
    #[prop(optional)]
    placed: bool,
) -> impl IntoView {
    let Shell {
        mode,
        set_mode,
        connected,
        read_only,
        viewer,
        sign_out,
        brand,
        set_trouble,
        chosen_range,
        custom,
        set_custom,
        open,
        wants_time,
        apply_preset,
        apply_range,
        ..
    } = shell();
    let SurfaceScope {
        draft,
        state: surface,
        writing,
        depth,
        ..
    } = scope();
    let apply_preset = move |seconds: i64| apply_preset.run(seconds);
    let apply_range = move |range: TimeRange| apply_range.run(range);
    // Edit composes the outermost surface only.
    let composing = move || depth == 0 && mode.get() == Mode::Composing;
    let editable = move || draft.with(|draft| draft.editable());
    let wants_time = move || !placed && wants_time.get();

    // The bar wraps when its contents outgrow a line, and the catalogue sticks
    // below it, so its height is measured rather than assumed. Written to
    // `<body>`, not `<html>`: an attribute change on `<html>` is how a theme
    // switch is noticed, and the bar's height is not one.
    let bar_ref: NodeRef<html::Header> = NodeRef::new();
    Effect::new(move |_| {
        if placed {
            return;
        }
        let Some(header) = bar_ref.get() else { return };
        let publish = Closure::<dyn FnMut()>::new(move || {
            let Some(header) = bar_ref.get_untracked() else {
                return;
            };
            let height = header.get_bounding_client_rect().height();
            if let Some(body) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.body())
            {
                let _ = body
                    .style()
                    .set_property("--hlin-bar-height", &format!("{height}px"));
            }
        });
        if let Ok(observer) = web_sys::ResizeObserver::new(publish.as_ref().unchecked_ref()) {
            observer.observe(&header);
        }
        publish.forget();
    });

    view! {
        <header class="bar" node_ref=bar_ref>
            {move || brand.get().map(|brand| view! {
                <span class="brand">
                    {brand.logo.then(|| view! {
                        <img class="brand-logo" src=crate::brand::LOGO_PATH alt="" />
                    })}
                    <strong class="brand-name">{brand.name}</strong>
                </span>
            })}
            <span class="tagline">{move || draft.with(|draft| draft.title().to_string())}</span>

            <Show when=wants_time>
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
            </Show>

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
            // Composing is the surface's, and there is none on screen while a
            // page is open.
            <Show when=move || !placed && depth == 0 && !read_only.get() && open.with(Option::is_none)>
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

            {move || viewer.get().map(|who| view! { <span class="viewer">{who}</span> })}

            {move || sign_out.get().map(|path| view! {
                <button
                    class="mode sign-out"
                    on:click=move |_| {
                        let path = path.clone();
                        leptos::task::spawn_local(async move {
                            if let Err(reason) = crate::api::sign_out(&path).await {
                                set_trouble.set(Some(reason));
                            }
                        });
                    }
                >
                    "Sign out"
                </button>
            })}

            // The front end's own, if it brought any: a theme switch, say.
            // Drawn as given; the shell neither reads nor styles it.
            {extra.map(|extra| view! { <div class="extra">{extra.run()}</div> })}
        </header>
    }
}

/// One surface: its panels on the grid, and, while composing, the catalogue to
/// add from.
///
/// Its panels and shell components each sit in their own cells: a bar, a
/// content component showing a page, a page selector. Only the outermost
/// surface is composed; a surface opened in a content component is watched.
///
/// Drawn inside [`App`], which provides the [`Shell`] it reads, and inside a
/// surface, which provides the layout it draws.
#[component]
pub fn SurfaceView(
    /// What draws the panels.
    drawer: Drawer,
) -> impl IntoView {
    let Shell {
        catalog,
        choices,
        mode,
        grace,
        read_only,
        modules,
        open,
        reapply,
        ..
    } = shell();
    let SurfaceScope {
        depth,
        state: surface,
        draft,
        set_draft,
        save,
        act,
        ..
    } = scope();
    let save = move || save.run(());
    let reapply = move || reapply.run(());
    let act = move |who: String, intent: hlin_view::Intent| act.run((who, intent));
    let composing = move || depth == 0 && mode.get() == Mode::Composing;
    // Hidden rather than dropped while a page is open, so its panels, and what
    // their modules hold, are there to go back to. A surface inside another
    // is hidden with it.
    let hidden = move || depth == 0 && open.with(Option::is_some);

    let (search, set_search) = signal(String::new());
    let (gesture, set_gesture) = signal(Option::<Gesture>::None);
    let (cell, set_cell) = signal(0.0f64);
    let stage: NodeRef<html::Main> = NodeRef::new();

    // What the mounted pack answers to. Taken before the pack is moved into
    // the closures that draw with it.
    let offered = drawer.offers().join(" ");
    let item_drawer = drawer.clone();

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
        if depth != 0 || mode.get_untracked() != Mode::Composing {
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

    view! {

        // Hidden rather than dropped while a page is open, so its panels, and
        // what their modules hold, are there to go back to.
        <div class="stage" class:composing=composing hidden=hidden>
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
                                                    // A panel only its module draws has no kind to
                                                    // name; saying "module" says who draws it.
                                                    <span class="offer-kind">{panel.kind.clone().unwrap_or_else(|| "module".to_string())}</span>
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
                <Show when=move || draft.with(|draft| draft.document().items.is_empty())>
                    {move || view! { <Empty composing=composing() read_only=read_only.get() /> }}
                </Show>

                // One entry per panel, keyed, so a panel is created once and
                // then updated in place. It used to be one closure redrawing
                // every panel whenever the draft or the stream moved — every
                // pointer move of a drag, every frame — which was fine for
                // panels drawn from data and is fatal for a module: a frame
                // recreated is a module reloaded, and would never get as far as
                // saying `ready`.
                <For
                    each=move || draft.with(|draft| panel_keys(&draft.panels()))
                    key=|key| key.clone()
                    children=move |key: String| {
                        let drawer = drawer.clone();

                        // This panel, as the draft has it now.
                        let instance = Memo::new({
                            let key = key.clone();
                            move |_| draft.with(|draft| find_panel(&draft.panels(), &key))
                        });
                        // The same with its position set aside, which is what
                        // the heading and body are drawn from: a panel being
                        // dragged moves, and does not redraw what is in it.
                        let content = Memo::new(move |_| {
                            instance.get().map(|mut panel| {
                                panel.position = Placement::default();
                                panel
                            })
                        });
                        let entry = Memo::new(move |_| {
                            content.with(|panel| {
                                let panel = panel.as_ref()?;
                                catalog.with(|catalog| {
                                    catalogued(catalog, &panel.platform_id, &panel.panel_key)
                                })
                            })
                        });
                        let id = move || content.with(|panel| panel.as_ref().and_then(|panel| panel.id.clone()));

                        // What the bridge says, for a panel its module draws.
                        // `None` for every other panel.
                        let module = Memo::new(move |_| {
                            let id = id()?;
                            entry.with(|entry| entry.as_ref().and_then(|entry| entry.ui.clone()))?;
                            Some(modules.with(|views| views.get(&id).cloned().unwrap_or_default()))
                        });

                        // The module to mount, while one is wanted: until it
                        // is given up on. A memo, so the frame is recreated
                        // only when what it would load actually changes.
                        let mount = Memo::new(move |_| {
                            let view = module.get()?;
                            if view.fallen_back || matches!(view.state, hlin_view::PanelState::Unavailable(_)) {
                                return None;
                            }
                            let panel = content.get()?;
                            let entry = entry.get()?;
                            let ui = entry.ui.clone()?;
                            let limits = catalog.with(|catalog| {
                                catalog
                                    .iter()
                                    .find(|platform| platform.id == panel.platform_id)
                                    .and_then(|platform| platform.module_limits)
                            });
                            Some(Mount {
                                instance: panel.id.clone()?,
                                platform: panel.platform_id,
                                panel: panel.panel_key,
                                entry: ui.entry,
                                bridge: ui.bridge,
                                declares_data: entry.kind.is_some(),
                                page: false,
                                limits: limits.unwrap_or_default(),
                            })
                        });

                        // The frame's accessible name: the panel's title and
                        // its platform, as a person would say it.
                        let frame_title = Signal::derive(move || {
                            content.with(|panel| {
                                let Some(panel) = panel else {
                                    return String::new();
                                };
                                let platform = catalog.with(|catalog| {
                                    catalog
                                        .iter()
                                        .find(|platform| platform.id == panel.platform_id)
                                        .and_then(|platform| platform.name.clone())
                                });
                                let title = entry.with(|entry| title_of(panel, entry.as_ref()));
                                format!(
                                    "{title} — {}",
                                    platform.unwrap_or_else(|| panel.platform_id.clone())
                                )
                            })
                        });

                        let known = move || id().and_then(|id| surface.with(|state| state.panel(&id).cloned()));

                        // A panel the stream has never mentioned, on a stream
                        // that has stopped answering, is not loading. Nothing is
                        // coming, and a skeleton that never resolves tells a
                        // person the panel is slow when the shell is gone.
                        let abandoned = move |known: &Option<PanelView>| {
                            known.is_none()
                                && surface.with(|state| {
                                    state.given_up(Utc::now(), Duration::seconds(grace.get()))
                                })
                        };

                        // One word for the markup. A module panel's is its
                        // module's, from the bridge, until it falls back; then
                        // it is the data path's, like any other panel.
                        let drawn_state = move || match module.get() {
                            Some(view) if !view.fallen_back => state_name(view.state),
                            _ => {
                                let known = known();
                                match (&known, abandoned(&known)) {
                                    (Some(panel), _) => state_name(panel.state),
                                    (None, true) => "unavailable",
                                    (None, false) => "waiting",
                                }
                            }
                        };
                        let module_state = move || {
                            module.get().map(|view| {
                                if view.fallen_back {
                                    "fallback"
                                } else {
                                    state_name(view.state)
                                }
                            })
                        };
                        let module_cause = move || {
                            module.get().and_then(|view| match view.state {
                                hlin_view::PanelState::Unavailable(cause) => Some(cause.to_string()),
                                _ => None,
                            })
                        };
                        let placed = move || instance.with(|panel| panel.as_ref().map(|panel| panel.position).unwrap_or_default());

                        view! {
                            <section
                                class="panel"
                                class:dragging=move || {
                                    let id = id();
                                    gesture.with(|active| {
                                        active.as_ref().is_some_and(|active| Some(&active.instance) == id.as_ref())
                                    })
                                }
                                // Named and located in the markup, so a browser
                                // test can say "this panel moved" rather than
                                // parsing a style attribute for grid lines.
                                data-instance=move || id().unwrap_or_default()
                                data-panel=move || content.with(|panel| panel.as_ref().map(|panel| panel.reference()).unwrap_or_default())
                                data-x=move || placed().x.to_string()
                                data-y=move || placed().y.to_string()
                                data-w=move || placed().w.to_string()
                                data-h=move || placed().h.to_string()
                                data-state=drawn_state
                                data-module=module_state
                                data-module-cause=module_cause
                                style:grid-column=move || { let at = placed(); format!("{} / span {}", at.x + 1, at.w) }
                                style:grid-row=move || { let at = placed(); format!("{} / span {}", at.y + 1, at.h) }
                            >
                                {move || {
                                    let drawer = drawer.clone();
                                    let for_data = drawer.clone();
                                    let Some(instance) = content.get() else {
                                        return ().into_any();
                                    };
                                    let catalogued = entry.get();
                                    let named = instance.id.clone().unwrap_or_default();
                                    let reference = instance.reference();
                                    let known = known();
                                    let abandoned = abandoned(&known);

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
                                        hlin_view::Emit::to(move |intent| act(who.clone(), intent))
                                    };

                                    let head = view! {
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
                                    };

                                    let data = move || view! {
                                        <PanelBody
                                            drawer=for_data.clone()
                                            known=known.clone()
                                            abandoned=abandoned
                                            kind_override=instance.kind_override.clone()
                                            catalogued=catalogued.clone()
                                            controls=controls.clone()
                                            emit=emit.clone()
                                        />
                                    };

                                    let body = match module.get() {
                                        // The module draws it; the frame follows this.
                                        Some(view) if !view.fallen_back => match view.state {
                                            hlin_view::PanelState::Unavailable(cause) => view! {
                                                <ModuleUnavailable
                                                    drawer=drawer.clone()
                                                    cause=cause
                                                    instance=named.clone()
                                                    fallen_back=false
                                                />
                                            }.into_any(),
                                            _ => ().into_any(),
                                        },
                                        // Drawn by Hlin, and saying why.
                                        Some(view) => view! {
                                            {data()}
                                            <ModuleUnavailable
                                                drawer=drawer.clone()
                                                cause=match view.state {
                                                    hlin_view::PanelState::Unavailable(cause) => cause,
                                                    _ => hlin_view::Cause::Unreachable,
                                                }
                                                instance=named.clone()
                                                fallen_back=true
                                            />
                                        }.into_any(),
                                        None => data().into_any(),
                                    };

                                    view! { {head} {body} }.into_any()
                                }}

                                // What the module asked to have said, in its
                                // own panel's frame and nowhere else, as text,
                                // and labelled as its platform's: the one place
                                // the shell shows a platform's own words
                                // (HLIN-S-0007, `notice`).
                                {move || {
                                    let notice = module.get()?.notice?;
                                    let platform = content.with(|panel| {
                                        let panel = panel.as_ref()?;
                                        catalog.with(|catalog| {
                                            catalog
                                                .iter()
                                                .find(|platform| platform.id == panel.platform_id)
                                                .map(|platform| platform.name.clone().unwrap_or_else(|| platform.id.clone()))
                                        })
                                    })?;
                                    let level = match notice.level {
                                        hlin_bridge::NoticeLevel::Info => "info",
                                        hlin_bridge::NoticeLevel::Warning => "warning",
                                        hlin_bridge::NoticeLevel::Error => "error",
                                    };
                                    Some(view! {
                                        <p class="module-notice" data-level=level role="status">
                                            <span class="module-notice-from">{format!("{platform} says")}</span>
                                            <span class="module-notice-text">{notice.text}</span>
                                        </p>
                                    })
                                }}

                                {move || mount.get().map(|mount| view! {
                                    <ModuleFrame mount=mount title=frame_title />
                                })}

                                <Show when=move || composing()>
                                    <span
                                        class="corner"
                                        on:pointerdown=move |event: PointerEvent| {
                                            if let Some(who) = id() {
                                                begin(event, who, GestureKind::Resize);
                                            }
                                        }
                                    />
                                </Show>
                            </section>
                        }
                    }
                />

                // The shell's own components on this surface, each in its
                // cells: a bar, a content component, a page selector. Keyed
                // like the panels, so each is created once and a content
                // component keeps the page it shows while the draft moves.
                <For
                    each=move || draft.with(|draft| component_keys(draft.document()))
                    key=|key| key.clone()
                    children=move |key: String| {
                        let drawer = item_drawer.clone();
                        let item = Memo::new({
                            let key = key.clone();
                            move |_| draft.with(|draft| find_component(draft.document(), &key))
                        });
                        // What it is, with its position set aside: moving it
                        // does not redraw it.
                        let what = Memo::new(move |_| {
                            item.get().map(|mut item| {
                                *item.position_mut() = Placement::default();
                                item
                            })
                        });
                        let placed = move || item.with(|item| item.as_ref().map(LayoutItem::position).unwrap_or_default());
                        view! {
                            <section
                                class="item"
                                data-item=key.clone()
                                data-type=move || what.with(|item| item.as_ref().map(LayoutItem::type_name).unwrap_or_default())
                                data-x=move || placed().x.to_string()
                                data-y=move || placed().y.to_string()
                                data-w=move || placed().w.to_string()
                                data-h=move || placed().h.to_string()
                                style:grid-column=move || { let at = placed(); format!("{} / span {}", at.x + 1, at.w) }
                                style:grid-row=move || { let at = placed(); format!("{} / span {}", at.y + 1, at.h) }
                            >
                                {move || match what.get() {
                                    Some(LayoutItem::Bar(_)) => view! { <Bar placed=true /> }.into_any(),
                                    Some(LayoutItem::Content(content)) => view! {
                                        <Content
                                            id=content.id.clone().unwrap_or_default()
                                            initial=content.initial.clone()
                                            drawer=drawer.clone()
                                        />
                                    }
                                    .into_any(),
                                    // A page selector is drawn by HLIN-T-0114;
                                    // until then it holds its cells.
                                    _ => ().into_any(),
                                }}
                            </section>
                        }
                    }
                />

                // While a panel is dragged or resized, a transparent sheet
                // over the whole grid. A frame is another document: a pointer
                // crossing one stops reaching this page, the window listeners
                // stop hearing the gesture, and the panel sticks mid-drag. The
                // shield keeps every pointer event on the page until it ends.
                <Show when=move || gesture.with(Option::is_some)>
                    <div class="drag-shield"></div>
                </Show>
            </main>
        </div>
    }
}

/// A platform's page, open at full width (HLIN-S-0007, *Pages*).
///
/// The same kind of frame as a panel's module, with the same bridge and the
/// same states, and `init.page` set. Unlike a panel it has nothing to fall
/// back to: a page whose module is unavailable says so, and offers to try it
/// again, which is the only way `unavailable` recovers.
#[component]
fn PageView(
    drawer: Drawer,
    target: PageTarget,
    catalog: ReadSignal<Vec<CatalogPlatform>>,
    catalog_known: ReadSignal<bool>,
    modules: RwSignal<BTreeMap<String, ModuleView>>,
) -> impl IntoView {
    // This opening's own instance: a page opened again is a new frame, and a
    // new instance, like a panel put on a surface twice.
    let instance = format!("page-{:x}", (js_sys::Math::random() * 2f64.powi(52)) as u64);

    let found = Memo::new({
        let target = target.clone();
        move |_| catalog.with(|catalog| route::page_entry(catalog, &target))
    });
    let module = Memo::new({
        let instance = instance.clone();
        move |_| modules.with(|views| views.get(&instance).cloned().unwrap_or_default())
    });

    // The module to mount, while one is wanted: until it is given up on.
    let mount = Memo::new({
        let instance = instance.clone();
        move |_| {
            let page = found.get()?;
            if matches!(module.get().state, hlin_view::PanelState::Unavailable(_)) {
                return None;
            }
            Some(Mount {
                platform: page.platform,
                panel: page.path,
                instance: instance.clone(),
                entry: page.ui.entry,
                bridge: page.ui.bridge,
                declares_data: false,
                page: true,
                limits: page.limits.unwrap_or_default(),
            })
        }
    });

    on_cleanup({
        let instance = instance.clone();
        move || {
            modules.update(|views| {
                views.remove(&instance);
            });
        }
    });

    // Not a page this shell can open: a platform that offers none by that
    // path, or an entry that is only a link. Said once the catalogue is known,
    // and never before, or every link to a page would flash it.
    let unknown = move || catalog_known.get() && found.with(Option::is_none);

    let state = move || {
        if !catalog_known.get() {
            "loading"
        } else if unknown() {
            "unavailable"
        } else {
            state_name(module.get().state)
        }
    };
    let cause = move || {
        if unknown() {
            return Some(hlin_view::Cause::Unknown.to_string());
        }
        match module.get().state {
            hlin_view::PanelState::Unavailable(cause) if found.with(Option::is_some) => {
                Some(cause.to_string())
            }
            _ => None,
        }
    };
    let label = Memo::new({
        let target = target.clone();
        move |_| {
            found.with(|page| {
                page.as_ref()
                    .map_or_else(|| target.path.clone(), |page| page.label.clone())
            })
        }
    });
    let platform = Memo::new({
        let target = target.clone();
        move |_| {
            found.with(|page| {
                page.as_ref().map_or_else(
                    || target.platform.clone(),
                    |page| page.platform_name.clone(),
                )
            })
        }
    });
    let frame_title = Signal::derive(move || format!("{} — {}", label.get(), platform.get()));
    let retried = instance.clone();

    view! {
        <section
            class="page"
            data-page=format!("{}/{}", target.platform, target.path)
            data-instance=instance.clone()
            data-state=state
            data-module=move || found.with(Option::is_some).then(|| state_name(module.get().state))
            data-module-cause=cause
        >
            <header class="page-head">
                <h2>{label}</h2>
                <span class="page-platform">{platform}</span>
            </header>

            // What the module asked to have said, as the platform's, exactly
            // as a panel's module's notice is.
            {move || {
                let notice = module.get().notice?;
                let level = match notice.level {
                    hlin_bridge::NoticeLevel::Info => "info",
                    hlin_bridge::NoticeLevel::Warning => "warning",
                    hlin_bridge::NoticeLevel::Error => "error",
                };
                Some(view! {
                    <p class="module-notice" data-level=level role="status">
                        <span class="module-notice-from">{format!("{} says", platform.get())}</span>
                        <span class="module-notice-text">{notice.text}</span>
                    </p>
                })
            }}

            {move || {
                let drawer = drawer.clone();
                if unknown() {
                    let drawn = plan(
                        hlin_view::PanelState::Unavailable(hlin_view::Cause::Unknown),
                        Kind::Raw,
                        None,
                    );
                    return view! {
                        {drawer.draw(&drawn, None)}
                        <p class="detail module-note">
                            {format!("There is no page here that {} can open.", crate::brand::name())}
                        </p>
                    }
                    .into_any();
                }
                match module.get().state {
                    hlin_view::PanelState::Unavailable(cause) => view! {
                        <ModuleUnavailable
                            drawer=drawer
                            cause=cause
                            instance=retried.clone()
                            fallen_back=false
                        />
                    }
                    .into_any(),
                    _ => ().into_any(),
                }
            }}

            {move || mount.get().map(|mount| view! {
                <ModuleFrame mount=mount title=frame_title />
            })}
        </section>
    }
}

/// Whether a click on a link is a plain one, which the shell handles itself,
/// rather than one asking the browser for a new tab or window, which it
/// leaves to the browser and the link's own address.
fn plain_click(event: &leptos::ev::MouseEvent) -> bool {
    event.button() == 0
        && !event.ctrl_key()
        && !event.meta_key()
        && !event.shift_key()
        && !event.alt_key()
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
        .and_then(|panel| panel.kind.clone())
        .unwrap_or_else(|| "raw".to_string());
    // A panel only its module draws has no kinds to choose between.
    let drawn_by_shell = catalogued.as_ref().is_none_or(|panel| panel.kind.is_some());
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
                hidden=!drawn_by_shell
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
                <p class="detail">{format!("{} is not responding", crate::brand::name())}</p>
            })}
        }
        .into_any();
    };

    // The viewer's choice, then the platform's default, then whatever the
    // vocabulary prefers for this envelope. An unknown name lands on `raw`, so
    // a panel always draws.
    let kind = kind_override
        .as_deref()
        .or(catalogued.as_ref().and_then(|entry| entry.kind.as_deref()))
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

/// What a module panel shows when its module is unavailable.
///
/// Drawn through the pack like any other unavailable panel, with the cause in
/// words, and one way back: mounting the module again is the only recovery
/// from `unavailable` (specification HLIN-S-0007, *Panel states*), and doing it
/// in a loop by itself would hammer a platform that is already failing. So a
/// person asks. Where the panel fell back to its data, this is the line under
/// that data saying Hlin drew it, and why.
#[component]
fn ModuleUnavailable(
    drawer: Drawer,
    cause: hlin_view::Cause,
    instance: String,
    /// The panel's data is drawn above this.
    fallen_back: bool,
) -> impl IntoView {
    let why = match cause {
        hlin_view::Cause::Malformed => "this platform's module could not be used",
        _ => "this platform's module is not responding",
    };
    let retry = move |_| crate::frame::retry(&instance);

    if fallen_back {
        return view! {
            <p class="detail module-note">
                {format!("Drawn by {}: {why}. ", crate::brand::name())}
                <button class="retry-module" on:click=retry>"Try the module again"</button>
            </p>
        }
        .into_any();
    }

    let drawn = plan(hlin_view::PanelState::Unavailable(cause), Kind::Raw, None);
    view! {
        {drawer.draw(&drawn, None)}
        <p class="detail module-note">
            {format!("{}. ", capitalised(why))}
            <button class="retry-module" on:click=retry>"Try the module again"</button>
        </p>
    }
    .into_any()
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
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
    let path = current_path();
    let id = path.strip_prefix("/s/")?.trim_end_matches('/');
    (!id.is_empty()).then(|| id.to_string())
}

/// The path the address bar holds now, or nothing where there is no window.
fn current_path() -> String {
    web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default()
}

/// Put an address in the address bar as a new history entry, without
/// navigating: somewhere a person went, so back brings them back. Ignored on
/// failure for the same reason as [`show_address`].
fn push_address(path: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(history) = window.history() {
        let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
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

/// The key each panel on the surface is drawn under: its identity, or, for one
/// added a moment ago that the shell has not named yet, its place.
fn panel_keys(panels: &[hlin_stream::layout::PanelInstanceDocument]) -> Vec<String> {
    panels
        .iter()
        .enumerate()
        .map(|(index, panel)| panel_key(index, panel))
        .collect()
}

fn panel_key(index: usize, panel: &hlin_stream::layout::PanelInstanceDocument) -> String {
    panel
        .id
        .clone()
        .unwrap_or_else(|| format!("unsaved:{index}"))
}

/// The key each shell component on the surface is drawn under: its id, or,
/// for one with none yet, its place among the items.
fn component_keys(document: &LayoutDocument) -> Vec<String> {
    document
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.as_panel().is_none())
        .map(|(index, item)| component_key(index, item))
        .collect()
}

fn component_key(index: usize, item: &LayoutItem) -> String {
    item.id()
        .map_or_else(|| format!("unsaved-item:{index}"), str::to_string)
}

/// The shell component drawn under a key, if it is still on the surface.
fn find_component(document: &LayoutDocument, key: &str) -> Option<LayoutItem> {
    document
        .items
        .iter()
        .enumerate()
        .find(|(index, item)| item.as_panel().is_none() && component_key(*index, item) == key)
        .map(|(_, item)| item.clone())
}

/// The panel drawn under a key, if it is still on the surface.
fn find_panel(
    panels: &[hlin_stream::layout::PanelInstanceDocument],
    key: &str,
) -> Option<hlin_stream::layout::PanelInstanceDocument> {
    panels
        .iter()
        .enumerate()
        .find(|(index, panel)| panel_key(*index, panel) == key)
        .map(|(_, panel)| panel.clone())
}

/// The panel a module's `navigate` names, where this surface can show it
/// (HLIN-S-0007, `navigate`). A page is opened by the caller, not here.
///
/// A panel is opened by bringing it into view on this surface: the one other
/// than the asker's own where there are several, the asker's own otherwise.
/// Navigation grants nothing, so a module may name another platform's panel.
/// Anything this shell cannot open — a panel no platform offers, one not on
/// this surface — is ignored, and said on the console for whoever is building
/// the module, never to the person looking.
fn panel_to_open(
    draft: &LayoutDraft,
    catalog: &[CatalogPlatform],
    asker: &str,
    to: &hlin_bridge::Target,
) -> Option<String> {
    let Some(panel) = to.panel.as_deref() else {
        leptos::logging::warn!("a module asked to go nowhere in particular; ignored");
        return None;
    };
    if catalogued(catalog, &to.platform, panel).is_none() {
        leptos::logging::warn!(
            "a module asked to open `{}/{panel}`, which no platform offers; ignored",
            to.platform
        );
        return None;
    }
    let placed: Vec<String> = draft
        .panels()
        .iter()
        .filter(|placed| placed.platform_id == to.platform && placed.panel_key == panel)
        .filter_map(|placed| placed.id.clone())
        .collect();
    let target = placed
        .iter()
        .find(|id| id.as_str() != asker)
        .or_else(|| placed.first())
        .cloned();
    if target.is_none() {
        leptos::logging::warn!(
            "a module asked to open `{}/{panel}`, which is not on this surface; ignored",
            to.platform
        );
    }
    target
}

/// Scroll a panel on this surface into view, and mark it for a moment.
fn bring_into_view(instance: &str) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .query_selector(&format!("section.panel[data-instance=\"{instance}\"]"))
                .ok()
                .flatten()
        })
    else {
        return;
    };
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_behavior(web_sys::ScrollBehavior::Smooth);
    options.set_block(web_sys::ScrollLogicalPosition::Center);
    element.scroll_into_view_with_scroll_into_view_options(&options);
    // Marked for a moment, so a person sees which panel they were taken to.
    let _ = element.class_list().add_1("navigated");
    leptos::task::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(1_500).await;
        let _ = element.class_list().remove_1("navigated");
    });
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
