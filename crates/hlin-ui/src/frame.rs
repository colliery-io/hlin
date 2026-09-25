//! Hosting a platform's module in a sandboxed frame: the parent end of the
//! bridge (specification HLIN-S-0007).
//!
//! The DOM half of [`crate::bridge`], and deliberately only wiring: every
//! decision — which frame sent a message, what state a quiet module is in,
//! whether a request is one too many, where a `fetch` may go, which modules
//! hear a change, which frame the budget unmounts — is made there and tested
//! there. What is here is the part that cannot be tested without a browser:
//! creating the frame, one `message` listener for the whole page, one clock,
//! the observers that say where a panel is, and the requests to `/p/` and to
//! the shell's relay, and the reading of a streamed response as fast as its
//! module asks and no faster.
//!
//! Frames are created imperatively rather than through the view, for one
//! reason the specification insists on: a frame leaving the surface is removed
//! from the registry *before* it is removed from the document, so a message
//! already in flight from it is dropped. Only code that owns both steps can
//! promise their order. The budget leans on the same property: a frame
//! unmounted to stay within it keeps its panel, its observers and its last
//! state, and only its document goes.
//!
//! The page's state lives in one thread-local, because the browser is single
//! threaded and every entry point here is a browser callback. Nothing that
//! re-enters it (posting a message, publishing a state to the view, calling
//! the app back) is done while it is borrowed.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use hlin_bridge::{
    ChangeSource, Chunk, Context, End, EndError, Envelope, Heartbeat, IdMint, Init, Limits, Method,
    ModuleChanged, ModuleMessage, NoticeLevel, Refusal, Response, Scheme, Selections, ShellChanged,
    ShellMessage, Suspend, Target, Theme, TimeRange, Viewer, Visibility,
};
use hlin_stream::layout::ModuleLimits;
use hlin_stream::streamed::{Decoder, Ended, Frame, STREAM_HEADER};
use hlin_stream::{ChangeOrigin, ChangedFrame};
use hlin_view::{Cause, PanelState};
use leptos::prelude::*;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};

use crate::bridge::{self, Allowance, Due, Liveness, Mounted, Registered, Registry, Room};

/// What the surface draws for one module panel.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleView {
    /// Where the module is, from the bridge.
    pub state: PanelState,
    /// The module is unavailable for a reason about itself, and the panel
    /// declares data, so the shell draws that instead.
    pub fallen_back: bool,
    /// What the module last asked to have shown in its panel's frame, if
    /// anything.
    pub notice: Option<ModuleNotice>,
}

impl Default for ModuleView {
    fn default() -> Self {
        Self {
            state: PanelState::Loading,
            fallen_back: false,
            notice: None,
        }
    }
}

/// A module's `notice`, as the shell shows it: plain text, already cut to
/// length, drawn as the platform's in that panel's frame and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNotice {
    /// How serious the module says it is.
    pub level: NoticeLevel,
    /// One line, at most [`hlin_bridge::NOTICE_MAX_CHARS`] characters.
    pub text: String,
}

/// Something a module asked the shell to do that only the app can: change
/// what the surface shows, or go somewhere.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// `set-param` or `set-range`, as the intent a component in the same panel
    /// would have emitted, so it takes the same road: stored with the layout,
    /// sent to the shell, and back to every module as `context`.
    Intent {
        /// The panel instance whose module asked.
        instance: String,
        /// What it asked for.
        intent: hlin_view::Intent,
    },
    /// `navigate`: open a panel or a page.
    Navigate {
        /// The panel instance whose module asked.
        instance: String,
        /// Where to.
        to: Target,
    },
}

/// One panel's parameters, as every module on the surface is told them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelContext {
    /// What the layout holds for the panel.
    pub selections: Selections,
    /// The parameter ids the panel declares: the only ones its module is
    /// told, and the only ones it may set.
    pub declared: BTreeSet<String>,
}

/// Everything needed to mount one panel's module, or one page's, and nothing
/// that changes while it is mounted: a change to any of this is a different
/// module, and remounts it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    /// The platform whose module it is.
    pub platform: String,
    /// The panel key, or the navigation entry's path for a page.
    pub panel: String,
    /// The panel instance, or the page's own for as long as it is open.
    pub instance: String,
    /// The entry document, relative to the platform's base.
    pub entry: String,
    /// The bridge major the manifest declared.
    pub bridge: u32,
    /// Whether the panel declares data to fall back to. Never for a page: a
    /// page has no shell-drawn fallback, and is `unavailable` and says so.
    pub declares_data: bool,
    /// Whether this is a navigation entry's page, open at full width, rather
    /// than a panel (*Pages*). A page is told so in `init.page`, is never
    /// unmounted to meet the budget, and so never has anything `restored`.
    pub page: bool,
    /// The limits the platform's modules run within.
    pub limits: ModuleLimits,
}

impl Mount {
    /// Where the frame loads its document: the shell's copy of the entry, with
    /// the instance in the fragment so a module can say which it is before
    /// `init` arrives. Not a secret; nothing relies on it.
    fn source(&self) -> String {
        format!("/m/{}{}#i={}", self.platform, self.entry, self.instance)
    }
}

/// One module panel, mounted or waiting to be.
struct Host {
    mount: Mount,
    /// Which mounting this is, so an answer to a request made by a frame since
    /// torn down and remounted is not delivered to its successor.
    serial: u64,
    container: web_sys::Element,
    /// The frame, while one is mounted.
    iframe: Option<web_sys::HtmlIFrameElement>,
    title: String,
    liveness: Liveness,
    allowance: Allowance,
    ids: IdMint,
    /// Whether the panel is in the viewport, as the observer last said.
    in_view: bool,
    /// Whether it is within the mounting margin of the viewport.
    near: bool,
    /// When it was last in the viewport, for the budget.
    last_seen: f64,
    /// Since when the module has been asked to `suspend`, before its frame is
    /// unmounted to stay within the budget.
    suspending: Option<f64>,
    /// Near the viewport and wanting a frame, but the budget is full: mounted
    /// once another frame has left the document.
    waiting: bool,
    /// The context this frame was last told, in `init` or `context`.
    told: Option<Context>,
    /// The theme this frame was last told.
    told_theme: Option<Theme>,
    /// What the module asked to have shown in its panel's frame.
    notice: Option<ModuleNotice>,
    observers: Vec<web_sys::IntersectionObserver>,
    /// Kept alive for as long as the observers and the frame can call them.
    _callbacks: Vec<Closure<dyn FnMut(js_sys::Array)>>,
    _onload: Option<Closure<dyn FnMut()>>,
}

impl Host {
    fn view(&self) -> ModuleView {
        let state = self.liveness.state();
        ModuleView {
            state,
            fallen_back: bridge::falls_back(state, self.mount.declares_data),
            notice: self.notice.clone(),
        }
    }

    /// Whether the module is running and talking: told anything now, it will
    /// hear it.
    fn listening(&self) -> bool {
        self.iframe.is_some()
            && self.suspending.is_none()
            && matches!(self.liveness.state(), PanelState::Ready | PanelState::Stale)
    }
}

/// What every module on this surface is told about where it is.
#[derive(Debug, Clone, Default)]
struct Surroundings {
    viewer: Option<String>,
    read_only: bool,
    time_range: Option<TimeRange>,
    generation: u64,
    panels: BTreeMap<String, PanelContext>,
}

impl Surroundings {
    fn context_for(&self, instance: &str) -> Context {
        let panel = self.panels.get(instance);
        Context {
            time_range: self.time_range,
            params: panel
                .map(|panel| bridge::declared_only(&panel.selections, &panel.declared))
                .unwrap_or_default(),
            generation: self.generation,
        }
    }
}

type AskedHandler = Rc<dyn Fn(Asked)>;

#[derive(Default)]
struct Page {
    registry: Registry<JsValue>,
    hosts: BTreeMap<String, Host>,
    published: Option<RwSignal<BTreeMap<String, ModuleView>>>,
    surroundings: Surroundings,
    serial: u64,
    started: bool,
    /// This page load, so the shell's echo of a change it relayed is known as
    /// its own.
    page_id: String,
    /// The surface on screen, which the shell's relay is addressed by.
    surface: Option<String>,
    /// Where `set-param`, `set-range` and `navigate` go.
    asked: Option<AskedHandler>,
    /// What each module handed back at its last `suspend`, kept for the life
    /// of the page and never sent anywhere but back to it (*Budget*).
    restored: BTreeMap<String, Vec<u8>>,
    /// Modules given up on as unreachable, by instance, with their platform
    /// and panel: remounted on that panel's next change from the platform.
    given_up: BTreeMap<String, (String, String)>,
    /// Streamed responses being read, by a number of the page's own: a
    /// module's `fetch` id is only unique within its own mounting.
    flows: BTreeMap<u64, Flow>,
    flow_serial: u64,
    /// How many module streams the whole page may hold open, from the
    /// protocol it was served over (*Streaming*, *Connections*).
    stream_cap: usize,
}

/// One streamed response, between the page's reads and its module's `pull`s.
struct Flow {
    instance: String,
    /// The mounting that asked, so a remounted frame neither counts nor
    /// feeds its predecessor's streams.
    serial: u64,
    /// The module's `fetch` id, which every `chunk` and `end` names.
    re: String,
    outbox: bridge::Outbox,
    /// Aborting it aborts the request, and the shell drops the platform's
    /// connection.
    abort: web_sys::AbortController,
    /// Wakes the reading task when it is waiting for credit.
    wake: Option<futures::channel::oneshot::Sender<()>>,
    /// Set when `end` has been sent, by whichever side ended it: the page
    /// never says anything after it.
    ended: bool,
}

impl Page {
    /// Streams open in one mounting, and on the whole page.
    fn open_streams(&self, instance: &str, serial: u64) -> (usize, usize) {
        let open = self.flows.values().filter(|flow| !flow.ended);
        let (mut here, mut all) = (0, 0);
        for flow in open {
            all += 1;
            if flow.instance == instance && flow.serial == serial {
                here += 1;
            }
        }
        (here, all)
    }

    /// The open stream this mounting's module calls `re`.
    fn flow_named(&self, instance: &str, serial: u64, re: &str) -> Option<u64> {
        self.flows
            .iter()
            .find(|(_, flow)| {
                !flow.ended && flow.instance == instance && flow.serial == serial && flow.re == re
            })
            .map(|(id, _)| *id)
    }
}

thread_local! {
    static PAGE: RefCell<Page> = RefCell::new(Page::default());
}

fn now() -> f64 {
    js_sys::Date::now()
}

fn document_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_none_or(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

// -- Starting --------------------------------------------------------------

/// Begin hosting modules on this page, publishing each one's state to
/// `published`. Idempotent.
///
/// One `message` listener for the whole page, not one per frame: the
/// registry, not the listener, is what attributes a message, and a listener
/// per frame would be one more thing to remove in the right order.
pub fn start(published: RwSignal<BTreeMap<String, ModuleView>>) {
    let first = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.published = Some(published);
        if page.page_id.is_empty() {
            page.page_id = format!("p-{:x}", (js_sys::Math::random() * 2f64.powi(52)) as u64);
        }
        !std::mem::replace(&mut page.started, true)
    });
    if !first {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };

    let protocol = window
        .performance()
        .map(|performance| performance.get_entries_by_type("navigation"))
        .and_then(|entries| {
            js_sys::Reflect::get(&entries.get(0), &JsValue::from_str("nextHopProtocol")).ok()
        })
        .and_then(|protocol| protocol.as_string())
        .unwrap_or_default();
    PAGE.with(|page| page.borrow_mut().stream_cap = bridge::page_stream_cap(&protocol));

    let heard = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(on_message);
    let _ = window.add_event_listener_with_callback("message", heard.as_ref().unchecked_ref());
    heard.forget();

    // A hidden tab throttles timers, so its modules are told and are not
    // judged by their silence (*Messages*, `heartbeat`).
    let shown = Closure::<dyn FnMut()>::new(|| {
        let ids: Vec<String> = PAGE.with(|page| page.borrow().hosts.keys().cloned().collect());
        for id in ids {
            seen(&id, None);
        }
    });
    if let Some(document) = window.document() {
        let _ = document
            .add_event_listener_with_callback("visibilitychange", shown.as_ref().unchecked_ref());
    }
    shown.forget();

    // A pack may draw differently for a dark system, and modules follow the
    // page. Nothing else moves the tokens: a pack's stylesheet is fixed for
    // the life of the page.
    if let Ok(Some(dark)) = window.match_media("(prefers-color-scheme: dark)") {
        let changed = Closure::<dyn FnMut()>::new(retheme);
        let _ = dark.add_event_listener_with_callback("change", changed.as_ref().unchecked_ref());
        changed.forget();
    }

    leptos::task::spawn_local(async {
        loop {
            gloo_timers::future::TimeoutFuture::new(bridge::INIT_RESEND_MS as u32).await;
            tick();
        }
    });
}

/// Who is looking, and whether writes will be refused, for every `init`.
pub fn set_viewer(name: Option<String>, read_only: bool) {
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.surroundings.viewer = name;
        page.surroundings.read_only = read_only;
    });
}

/// The surface on screen, which a module's `changed` is relayed through.
pub fn set_surface(surface: Option<String>) {
    PAGE.with(|page| page.borrow_mut().surface = surface);
}

/// Where a module's `set-param`, `set-range` and `navigate` are sent.
pub fn on_asked(handler: impl Fn(Asked) + 'static) {
    PAGE.with(|page| page.borrow_mut().asked = Some(Rc::new(handler)));
}

/// The surface's time range and each panel's parameters: for every `init`,
/// and as `context` to every running module whose view of them moved.
///
/// Diffed per frame, so a module hears exactly the changes that concern it —
/// its panel's own parameters, the range, the generation — and not every
/// redraw of the layout.
pub fn set_context(
    time_range: Option<TimeRange>,
    generation: u64,
    panels: BTreeMap<String, PanelContext>,
) {
    let ids: Vec<String> = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.surroundings.time_range = time_range;
        page.surroundings.generation = generation;
        page.surroundings.panels = panels;
        page.hosts.keys().cloned().collect()
    });
    for id in ids {
        tell_context(&id);
    }
}

/// Send a running module its context, if it differs from what it was told.
fn tell_context(instance: &str) {
    let owed = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let Page {
            hosts,
            surroundings,
            ..
        } = &mut *page;
        let host = hosts.get_mut(instance)?;
        if !host.listening() {
            return None;
        }
        let context = surroundings.context_for(instance);
        if host.told.as_ref() == Some(&context) {
            return None;
        }
        host.told = Some(context.clone());
        Some(context)
    });
    if let Some(context) = owed {
        post(instance, None, ShellMessage::Context(context));
    }
}

// -- The theme ---------------------------------------------------------------

/// The page's theme, as the mounted pack drew it: the chrome's colour roles
/// the pack filled (or the shell's defaults, where it filled none), and light
/// or dark as the page's own surface reads.
fn current_theme() -> Theme {
    let Some(window) = web_sys::window() else {
        return Theme::default();
    };
    let prefers_dark = window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    let style = window
        .document()
        .and_then(|document| document.document_element())
        .and_then(|root| window.get_computed_style(&root).ok().flatten());
    let Some(style) = style else {
        return Theme {
            scheme: if prefers_dark {
                Scheme::Dark
            } else {
                Scheme::Light
            },
            tokens: BTreeMap::new(),
        };
    };
    let tokens: BTreeMap<String, String> = bridge::THEME_TOKENS
        .iter()
        .filter_map(|name| {
            let value = style.get_property_value(name).ok()?;
            let value = value.trim();
            (!value.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect();
    let surface = tokens.get("--hlin-surface").cloned().unwrap_or_default();
    Theme {
        scheme: bridge::scheme_of(&surface, prefers_dark),
        tokens,
    }
}

/// The scheme or the tokens may have moved: tell every running module whose
/// theme differs.
fn retheme() {
    let theme = current_theme();
    let owed: Vec<String> = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.hosts
            .iter_mut()
            .filter(|(_, host)| host.listening() && host.told_theme.as_ref() != Some(&theme))
            .map(|(id, host)| {
                host.told_theme = Some(theme.clone());
                id.clone()
            })
            .collect()
    });
    for id in owed {
        post(&id, None, ShellMessage::Theme(theme.clone()));
    }
}

// -- Mounting --------------------------------------------------------------

/// Take charge of a panel's module, drawn into `container`.
///
/// Nothing is loaded yet: the frame is created when the panel comes within
/// [`bridge::MOUNT_MARGIN`] of the viewport. A second call for an instance
/// already hosted only updates the frame's title.
pub fn host(container: web_sys::Element, mount: Mount, title: String) {
    if !hlin_bridge::SUPPORTED_BRIDGE_MAJORS.contains(&mount.bridge) {
        // Never mounted: the page could not talk to it.
        publish(
            &mount.instance,
            settled(
                PanelState::Unavailable(Cause::Malformed),
                mount.declares_data,
            ),
        );
        return;
    }

    let instance = mount.instance.clone();
    let known = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let Some(host) = page.hosts.get_mut(&instance) else {
            return false;
        };
        if host.title != title {
            if let Some(iframe) = &host.iframe {
                let _ = iframe.set_attribute("title", &title);
            }
            host.title = title.clone();
        }
        true
    });
    if known {
        return;
    }

    let near = {
        let id = instance.clone();
        Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            let close = intersecting(&entries);
            PAGE.with(|page| {
                if let Some(host) = page.borrow_mut().hosts.get_mut(&id) {
                    host.near = close;
                    // Gone from the margin before there was room: it no
                    // longer wants a frame.
                    host.waiting &= close;
                }
            });
            if close {
                attach(&id);
            }
        })
    };
    let within = {
        let id = instance.clone();
        Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            seen(&id, Some(intersecting(&entries)));
        })
    };

    let mut observers = Vec::new();
    for (callback, margin) in [(&near, bridge::MOUNT_MARGIN), (&within, "0px")] {
        let options = web_sys::IntersectionObserverInit::new();
        options.set_root_margin(margin);
        if let Ok(observer) = web_sys::IntersectionObserver::new_with_options(
            callback.as_ref().unchecked_ref(),
            &options,
        ) {
            observer.observe(&container);
            observers.push(observer);
        }
    }

    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.serial += 1;
        let serial = page.serial;
        page.hosts.insert(
            instance.clone(),
            Host {
                allowance: Allowance::new(
                    mount.limits.messages_per_second,
                    mount.limits.fetches_in_flight,
                ),
                mount,
                serial,
                container,
                iframe: None,
                title,
                liveness: Liveness::new(now()),
                ids: IdMint::shell(),
                in_view: false,
                near: false,
                last_seen: now(),
                suspending: None,
                waiting: false,
                told: None,
                told_theme: None,
                notice: None,
                observers,
                _callbacks: vec![near, within],
                _onload: None,
            },
        );
    });
    publish(&instance, ModuleView::default());
}

/// Whether any of an observer's entries says the target is intersecting.
fn intersecting(entries: &js_sys::Array) -> bool {
    entries.iter().any(|entry| {
        entry
            .dyn_into::<web_sys::IntersectionObserverEntry>()
            .is_ok_and(|entry| entry.is_intersecting())
    })
}

/// The panel came near the viewport: create its frame, if it has none.
///
/// A frame unmounted to stay within the budget comes back the same way, as a
/// new document: `loading` again, a new handshake, and whatever it handed back
/// at `suspend` in `init.restored`.
///
/// At the budget, room is made first and the frame waits for it: another is
/// suspended and leaves the document, and then this one is mounted, so the
/// document never holds more than the budget on the way (*Budget*).
fn attach(instance: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };

    let room = PAGE.with(|page| {
        let page = page.borrow();
        let host = page.hosts.get(instance)?;
        if host.iframe.is_some() {
            return None;
        }
        // A page is never counted against the budget, and never waits.
        if host.mount.page {
            return Some(Room::Now);
        }
        Some(bridge::room_for(
            &budgeted(&page),
            bridge::FRAME_BUDGET,
            host.in_view,
        ))
    });
    match room {
        None => return,
        Some(Room::Now) => {}
        Some(Room::After(going)) => {
            PAGE.with(|page| {
                if let Some(host) = page.borrow_mut().hosts.get_mut(instance) {
                    host.waiting = true;
                }
            });
            for id in going {
                let_go(&id);
            }
            return;
        }
    }

    let prepared = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        if page.hosts.get(instance)?.iframe.is_some() {
            return None;
        }
        page.serial += 1;
        let serial = page.serial;
        let host = page.hosts.get_mut(instance)?;
        let iframe = document
            .create_element("iframe")
            .ok()?
            .dyn_into::<web_sys::HtmlIFrameElement>()
            .ok()?;

        // `allow-scripts allow-forms` and nothing more: never
        // `allow-same-origin`, whose absence is the opaque origin that keeps a
        // module away from the page, its cookies and its storage (REQ-1.1).
        // No `loading="lazy"`: the page already mounts only near the viewport,
        // and a load event the browser defers is a `ready` timeout the module
        // did not earn.
        let _ = iframe.set_attribute("sandbox", "allow-scripts allow-forms");
        let _ = iframe.set_attribute("allow", "");
        let _ = iframe.set_attribute("referrerpolicy", "no-referrer");
        let _ = iframe.set_attribute("title", &host.title);
        iframe.set_src(&host.mount.source());

        let loaded = {
            let id = instance.to_string();
            Closure::<dyn FnMut()>::new(move || on_load(&id))
        };
        iframe.set_onload(Some(loaded.as_ref().unchecked_ref()));
        host._onload = Some(loaded);

        // A new document, judged from now: its liveness, its allowance and
        // what it has been told all start again. The clock for the `ready`
        // timeout starts here rather than when the panel was first placed, so
        // a panel far down a surface is not given up on before it is ever
        // mounted.
        let at = now();
        host.waiting = false;
        host.serial = serial;
        host.liveness = Liveness::new(at);
        host.liveness
            .visible(host.in_view && document_visible(), at);
        host.allowance = Allowance::new(
            host.mount.limits.messages_per_second,
            host.mount.limits.fetches_in_flight,
        );
        host.suspending = None;
        host.told = None;
        host.told_theme = None;
        host.notice = None;

        let _ = host.container.append_child(&iframe);
        // The window exists once the frame is in the document, and it is the
        // same window after the frame navigates to its source, which is what
        // lets it be registered before anything in it can run.
        let window = iframe.content_window()?;
        let registered = Registered {
            platform: host.mount.platform.clone(),
            panel: host.mount.panel.clone(),
            instance: instance.to_string(),
            mounted_at: at,
        };
        let entry = format!("/m/{}{}", host.mount.platform, host.mount.entry);
        let view = host.view();
        host.iframe = Some(iframe);
        page.registry.register(JsValue::from(window), registered);
        Some((entry, serial, view))
    });

    let Some((entry, serial, view)) = prepared else {
        return;
    };
    publish(instance, view);
    keep_to_budget();

    // The frame's own load event cannot say how its document was answered, and
    // a 404 and a 502 both load *something*. The page asks for the entry itself,
    // beside the frame, so a module that was refused is `malformed` at once and
    // one whose platform is down is `unreachable`, rather than both waiting out
    // the `ready` timeout as the same thing. The frame's request revalidates
    // what this one fetched, so it costs a round trip, not the document twice.
    let id = instance.to_string();
    leptos::task::spawn_local(async move {
        let status = match gloo_net::http::Request::get(&entry).send().await {
            Ok(response) => response.status(),
            Err(_) => 0,
        };
        if let Some(cause) = bridge::entry_cause(status) {
            let changed = PAGE.with(|page| {
                let mut page = page.borrow_mut();
                let host = page
                    .hosts
                    .get_mut(&id)
                    .filter(|host| host.serial == serial)?;
                host.liveness.failed(cause);
                Some(host.view())
            });
            if let Some(view) = changed {
                conclude(&id, view);
            }
        }
    });
}

fn on_load(instance: &str) {
    PAGE.with(|page| {
        if let Some(host) = page.borrow_mut().hosts.get_mut(instance) {
            host.liveness.loaded(now());
        }
    });
    // `init` now rather than at the next tick.
    tick();
}

/// The panel came into or went out of view (`Some`), or the tab was shown or
/// hidden (`None`).
fn seen(instance: &str, in_view: Option<bool>) {
    let (changed, remount) = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let Some(host) = page.hosts.get_mut(instance) else {
            return (None, false);
        };
        let at = now();
        if let Some(in_view) = in_view {
            // Seen until the moment it leaves, and from the moment it comes
            // back: the budget's "least recently seen".
            if in_view || host.in_view {
                host.last_seen = at;
            }
            host.in_view = in_view;
            if in_view {
                // Back before its suspension ran out: it stays.
                host.suspending = None;
            }
        }
        let remount = host.in_view && host.iframe.is_none();
        let visible = host.in_view && document_visible();
        if host.iframe.is_none() || visible == host.liveness.is_visible() {
            return (None, remount);
        }
        host.liveness.visible(visible, at);
        (host.liveness.has_loaded().then_some(visible), remount)
    });
    if let Some(visible) = changed {
        post(
            instance,
            None,
            ShellMessage::Visibility(Visibility { visible }),
        );
    }
    if remount {
        attach(instance);
    }
}

/// Stop hosting a panel's module: out of the registry, then out of the
/// document. Called when its panel leaves the surface, and when the module is
/// given up on.
pub fn unmount(instance: &str) {
    end_streams_of(instance);
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.registry.remove(instance);
        if let Some(host) = page.hosts.remove(instance) {
            for observer in &host.observers {
                observer.disconnect();
            }
            if let Some(iframe) = &host.iframe {
                iframe.remove();
            }
        }
    });
    // Deferred, because this runs while the view is tearing a panel down.
    leptos::task::spawn_local(async { mount_waiting() });
}

/// Mount a module again after it was given up on: the only way `unavailable`
/// recovers (*Panel states*).
pub fn retry(instance: &str) {
    let published = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.given_up.remove(instance);
        page.published
    });
    if let Some(published) = published {
        published.update(|views| {
            views.remove(instance);
        });
    }
}

// -- The budget --------------------------------------------------------------

/// The frames in the document, as the budget weighs them: every panel's,
/// including one being suspended, and never a page's.
fn budgeted(page: &Page) -> Vec<Mounted> {
    page.hosts
        .iter()
        // A page is the one frame a person has open at full width: never
        // the budget's to take, and not counted against it.
        .filter(|(_, host)| host.iframe.is_some() && !host.mount.page)
        .map(|(id, host)| Mounted {
            instance: id.clone(),
            in_view: host.in_view,
            near: host.near,
            last_seen: host.last_seen,
            leaving: host.suspending.is_some(),
        })
        .collect()
}

/// Unmount what the budget says must go, out-of-view frames seen least
/// recently first (*Budget*, REQ-4.3).
fn keep_to_budget() {
    let doomed =
        PAGE.with(|page| bridge::over_budget(&budgeted(&page.borrow()), bridge::FRAME_BUDGET));
    for id in doomed {
        let_go(&id);
    }
}

/// Start taking one frame away for the budget.
///
/// A module that can hear is offered `suspend` first and unmounted when it
/// answers `state` or its deadline passes; one still loading has nothing to
/// keep and goes at once.
fn let_go(id: &str) {
    let asks = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let host = page.hosts.get_mut(id)?;
        if host.iframe.is_none() || host.suspending.is_some() {
            return None;
        }
        let asks = host.listening();
        if asks {
            host.suspending = Some(now());
        }
        Some(asks)
    });
    match asks {
        Some(true) => post(
            id,
            None,
            ShellMessage::Suspend(Suspend {
                deadline_ms: hlin_bridge::SUSPEND_DEADLINE_MS,
            }),
        ),
        Some(false) => detach(id),
        None => {}
    }
}

/// A frame left the document, or stayed after all: mount whatever was
/// waiting for room, those in view first.
fn mount_waiting() {
    let waiting: Vec<String> = PAGE.with(|page| {
        let page = page.borrow();
        let mut waiting: Vec<(&String, &Host)> = page
            .hosts
            .iter()
            .filter(|(_, host)| host.waiting && host.iframe.is_none())
            .collect();
        waiting.sort_by_key(|(_, host)| !host.in_view);
        waiting.into_iter().map(|(id, _)| id.clone()).collect()
    });
    for id in waiting {
        attach(&id);
    }
}

/// Take a frame's document away and keep its panel: out of the registry,
/// then out of the document, exactly as [`unmount`] does, but the panel keeps
/// its observers and its last state, held, until it comes near again.
///
/// A frame that came back into view while it was being suspended is left
/// where it is: the budget never takes what a person is looking at.
fn detach(instance: &str) {
    let staying = PAGE.with(|page| {
        page.borrow()
            .hosts
            .get(instance)
            .is_none_or(|host| host.in_view)
    });
    if !staying {
        end_streams_of(instance);
    }
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let Some(host) = page.hosts.get(instance) else {
            return;
        };
        if host.in_view {
            if let Some(host) = page.hosts.get_mut(instance) {
                host.suspending = None;
            }
            return;
        }
        page.registry.remove(instance);
        let Some(host) = page.hosts.get_mut(instance) else {
            return;
        };
        if let Some(iframe) = host.iframe.take() {
            iframe.set_onload(None);
            iframe.remove();
        }
        host._onload = None;
        host.suspending = None;
        host.told = None;
        host.told_theme = None;
    });
    mount_waiting();
}

// -- The clock ---------------------------------------------------------------

fn tick() {
    let at = now();
    let mut due = Vec::new();
    let mut changed = Vec::new();
    let mut expired = Vec::new();

    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        for (id, host) in page.hosts.iter_mut() {
            // Nothing to judge without a document: a panel not yet near the
            // viewport, or one the budget unmounted, holds its last state.
            if host.iframe.is_none() {
                continue;
            }
            if let Some(since) = host.suspending {
                if at - since >= hlin_bridge::SUSPEND_DEADLINE_MS as f64 {
                    expired.push(id.clone());
                }
                continue;
            }
            let before = host.liveness.state();
            if let Some(owed) = host.liveness.tick(at) {
                due.push((id.clone(), owed));
            }
            let after = host.liveness.state();
            if after != before {
                changed.push((id.clone(), host.view()));
            }
        }
    });

    for (id, owed) in due {
        match owed {
            Due::Init => {
                if let Some(init) = init_for(&id) {
                    post(&id, None, ShellMessage::Init(init));
                }
            }
            Due::Heartbeat(n) => post(&id, None, ShellMessage::Heartbeat(Heartbeat { n })),
        }
    }
    for (id, view) in changed {
        conclude(&id, view);
    }
    // The module kept nothing in time, or said nothing: it starts fresh.
    for id in expired {
        detach(&id);
    }
}

fn init_for(instance: &str) -> Option<Init> {
    let theme = current_theme();
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let restored = page.restored.get(instance).cloned();
        let Page {
            hosts,
            surroundings,
            ..
        } = &mut *page;
        let host = hosts.get_mut(instance)?;
        // A page is never suspended, so there is never anything to hand back.
        let restored = restored.filter(|_| !host.mount.page);
        let limits = host.mount.limits;
        let context = surroundings.context_for(instance);
        host.told = Some(context.clone());
        host.told_theme = Some(theme.clone());
        Some(Init {
            platform: host.mount.platform.clone(),
            panel: host.mount.panel.clone(),
            instance: instance.to_string(),
            page: host.mount.page,
            context,
            theme,
            viewer: Viewer {
                name: surroundings.viewer.clone(),
            },
            read_only: surroundings.read_only,
            limits: Limits {
                request_bytes: limits.request_bytes,
                response_bytes: limits.response_bytes,
                fetches_in_flight: u64::from(limits.fetches_in_flight),
                streams: u64::from(limits.streams),
            },
            restored,
        })
    })
}

// -- Publishing --------------------------------------------------------------

fn settled(state: PanelState, declares_data: bool) -> ModuleView {
    ModuleView {
        state,
        fallen_back: bridge::falls_back(state, declares_data),
        notice: None,
    }
}

fn publish(instance: &str, view: ModuleView) {
    let Some(published) = PAGE.with(|page| page.borrow().published) else {
        return;
    };
    let current = published.with_untracked(|views| views.get(instance).cloned());
    if current.as_ref() != Some(&view) {
        published.update(|views| {
            views.insert(instance.to_string(), view);
        });
    }
}

/// A module's state moved. An unavailable one is torn down at once, before
/// the view hears: it is not coming back without a remount, and a frame left
/// in place would go on spending a person's CPU on a module nobody can use.
///
/// Deferred a turn, because this can be reached from inside the frame's own
/// load callback, which tearing the frame down would drop while it ran.
fn conclude(instance: &str, view: ModuleView) {
    if matches!(view.state, PanelState::Unavailable(_)) {
        // An unreachable module comes back on its platform's next word about
        // its panel (*Panel states*); a malformed one does not, because
        // nothing about it will have changed.
        if view.state == PanelState::Unavailable(Cause::Unreachable) {
            PAGE.with(|page| {
                let mut page = page.borrow_mut();
                let origin = page
                    .hosts
                    .get(instance)
                    .map(|host| (host.mount.platform.clone(), host.mount.panel.clone()));
                if let Some(origin) = origin {
                    page.given_up.insert(instance.to_string(), origin);
                }
            });
        }
        let id = instance.to_string();
        leptos::task::spawn_local(async move {
            unmount(&id);
            publish(&id, view);
        });
        return;
    }
    publish(instance, view);
}

// -- Changes -----------------------------------------------------------------

/// The shell says a platform changed something: its own event stream, or a
/// module of it on any surface this shell serves (HLIN-S-0003, `changed`).
///
/// Passed to every running module of that platform that would care, as the
/// bridge's `changed`. A change this page relayed itself is skipped: its own
/// modules were told when it happened.
pub fn heard(frame: ChangedFrame) {
    let mine = PAGE.with(|page| {
        let page = page.borrow();
        frame.page.as_deref() == Some(page.page_id.as_str())
    });
    if mine {
        return;
    }
    let from = match frame.from {
        ChangeOrigin::Platform => ChangeSource::Platform,
        ChangeOrigin::Module => ChangeSource::Module,
    };
    tell_changed(&frame.platform, &frame.panel, &frame.selections, from, None);

    if frame.from == ChangeOrigin::Platform {
        let back: Vec<String> = PAGE.with(|page| {
            page.borrow()
                .given_up
                .iter()
                .filter(|(_, (platform, panel))| {
                    *platform == frame.platform && *panel == frame.panel
                })
                .map(|(id, _)| id.clone())
                .collect()
        });
        for id in back {
            retry(&id);
        }
    }
}

/// Tell every running module of a platform that something changed, except the
/// one that said so.
fn tell_changed(
    platform: &str,
    panel: &str,
    selections: &Selections,
    from: ChangeSource,
    except: Option<&str>,
) {
    let told: Vec<String> = PAGE.with(|page| {
        let page = page.borrow();
        page.hosts
            .iter()
            .filter(|(id, host)| {
                host.mount.platform == platform
                    && Some(id.as_str()) != except
                    && host.listening()
                    && bridge::hears(
                        selections,
                        &page.surroundings.context_for(id.as_str()).params,
                    )
            })
            .map(|(id, _)| id.clone())
            .collect()
    });
    for id in told {
        post(
            &id,
            None,
            ShellMessage::Changed(ShellChanged {
                panel: panel.to_string(),
                selections: selections.clone(),
                from,
            }),
        );
    }
}

/// A module said it wrote something. Its platform's other modules on this page
/// hear it now; everyone else's hear it through the shell, which is the only
/// thing that can reach them.
fn spread(instance: &str, platform: &str, change: ModuleChanged) {
    tell_changed(
        platform,
        &change.panel,
        &change.selections,
        ChangeSource::Module,
        Some(instance),
    );

    let (surface, page_id) = PAGE.with(|page| {
        let page = page.borrow();
        (page.surface.clone(), page.page_id.clone())
    });
    let Some(surface) = surface else {
        return;
    };
    let request = hlin_stream::ChangedRequest {
        platform: platform.to_string(),
        panel: change.panel,
        selections: change.selections,
        page: Some(page_id),
    };
    leptos::task::spawn_local(async move {
        let Ok(body) = serde_json::to_string(&request) else {
            return;
        };
        let sent = gloo_net::http::Request::post(&format!("/api/stream/{surface}/changed"))
            .header("content-type", "application/json")
            .body(body);
        if let Ok(sent) = sent {
            // Best-effort, like a platform's own event: a surface that misses
            // it is one refetch behind, and a module is never told it failed.
            let _ = sent.send().await;
        }
    });
}

// -- Messages ----------------------------------------------------------------

/// Send a module a message, transferring any bytes it carries.
fn post(instance: &str, re: Option<String>, message: ShellMessage) {
    let prepared = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let host = page.hosts.get_mut(instance)?;
        let window = host.iframe.as_ref()?.content_window()?;
        let id = host.ids.mint();
        let envelope = match re {
            Some(re) => Envelope::reply(id, re, message),
            None => Envelope::new(id, message),
        };
        Some((window, envelope))
    });
    let Some((window, envelope)) = prepared else {
        return;
    };
    let (value, transfer) = hlin_bridge::js::to_js(&envelope);
    // `*`, because an opaque origin cannot be named, and nothing sent here is
    // a secret its platform could not tell it anyway (REQ-2.2).
    let _ = window.post_message_with_transfer(&value, "*", &transfer);
}

/// What handling one message left to do once the page's state is let go.
enum Then {
    Nothing,
    Publish(ModuleView),
    /// The module said `ready`: publish, and bring it up to date with
    /// anything that moved since its `init` was written.
    Ready(ModuleView),
    Refuse(String, Refusal, &'static str),
    Carry(Carriage),
    Ask(Asked),
    SetParam(String, Vec<String>),
    Changed(String, ModuleChanged),
    /// The module answered `suspend`: keep this, and unmount it.
    Keep(Option<Vec<u8>>),
    /// Credit for a stream of this mounting's.
    Pull(u64, hlin_bridge::Pull),
    /// The module ended a stream of this mounting's.
    Cancel(u64, String),
}

/// A `fetch` the page will make.
struct Carriage {
    serial: u64,
    re: String,
    address: String,
    fetch: hlin_bridge::Fetch,
}

/// Room for streams, as the page stood when a message arrived.
#[derive(Debug, Clone, Copy)]
struct StreamRoom {
    /// Open in the frame that sent it.
    in_frame: usize,
    /// Open on the whole page.
    on_page: usize,
    /// The page's cap.
    page_cap: usize,
}

fn on_message(event: web_sys::MessageEvent) {
    let Some(source) = event.source() else {
        return;
    };
    let source = JsValue::from(source);
    // Anything that is not an envelope, and any type this page does not know,
    // is dropped without reply (REQ-2.3).
    let Some(envelope) = hlin_bridge::js::from_js::<ModuleMessage>(&event.data()).message() else {
        return;
    };
    let at = now();

    let handled = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        // Identity is the window the message came from, never anything in it.
        let instance = page.registry.source(&source)?.instance.clone();
        let serial = page.hosts.get(&instance)?.serial;
        let (in_frame, on_page) = page.open_streams(&instance, serial);
        let room = StreamRoom {
            in_frame,
            on_page,
            page_cap: page.stream_cap,
        };
        let host = page.hosts.get_mut(&instance)?;
        Some((instance, handle(host, envelope, at, room)))
    });

    let Some((instance, then)) = handled else {
        return;
    };
    match then {
        Then::Nothing => {}
        Then::Publish(view) => conclude(&instance, view),
        Then::Ready(view) => {
            // Handed back, so no longer the page's to keep: the next
            // `suspend` will say what to keep next time.
            PAGE.with(|page| page.borrow_mut().restored.remove(&instance));
            conclude(&instance, view);
            tell_context(&instance);
            retheme();
        }
        Then::Refuse(re, refusal, reason) => post(
            &instance,
            Some(re),
            ShellMessage::Response(refused(refusal, reason)),
        ),
        Then::Carry(carriage) if carriage.fetch.stream => {
            // Counted open from this moment, before anything else the module
            // sent is handled, so two asked for at once cannot both be the
            // last one allowed.
            let Some((flow, signal)) = open_flow(&instance, &carriage) else {
                return;
            };
            leptos::task::spawn_local(carry_stream(instance, carriage, flow, signal));
        }
        Then::Carry(carriage) => {
            leptos::task::spawn_local(carry(instance, carriage));
        }
        Then::Pull(serial, pull) => {
            PAGE.with(|page| {
                let mut page = page.borrow_mut();
                let flow = page
                    .flow_named(&instance, serial, &pull.re)
                    .and_then(|id| page.flows.get_mut(&id));
                if let Some(flow) = flow {
                    flow.outbox.grant(pull.bytes);
                    if let Some(wake) = flow.wake.take() {
                        let _ = wake.send(());
                    }
                }
            });
        }
        Then::Cancel(serial, re) => {
            let flow = PAGE.with(|page| page.borrow().flow_named(&instance, serial, &re));
            if let Some(flow) = flow {
                end_flow(flow, Some(EndError::Cancelled));
            }
        }
        Then::Ask(asked) => ask(asked),
        Then::SetParam(id, values) => {
            // Only a parameter the panel declares: anything else is not a
            // question this panel was ever asked, and is not the module's to
            // invent.
            let declared = PAGE.with(|page| {
                page.borrow()
                    .surroundings
                    .panels
                    .get(&instance)
                    .is_some_and(|panel| panel.declared.contains(&id))
            });
            if declared {
                ask(Asked::Intent {
                    instance,
                    intent: hlin_view::Intent::Select { param: id, values },
                });
            } else {
                leptos::logging::warn!(
                    "a module asked to set `{id}`, which its panel does not declare; ignored"
                );
            }
        }
        Then::Changed(platform, change) => spread(&instance, &platform, change),
        Then::Keep(blob) => {
            if let Some(blob) = blob {
                PAGE.with(|page| {
                    page.borrow_mut().restored.insert(instance.clone(), blob);
                });
            }
            detach(&instance);
        }
    }
}

/// Hand the app something only it can do.
fn ask(asked: Asked) {
    let handler = PAGE.with(|page| page.borrow().asked.clone());
    if let Some(handler) = handler {
        handler(asked);
    }
}

fn handle(host: &mut Host, envelope: Envelope<ModuleMessage>, at: f64, room: StreamRoom) -> Then {
    if matches!(host.liveness.state(), PanelState::Unavailable(_)) {
        return Then::Nothing;
    }

    // Stream credit does not count against the rate; everything else does,
    // and past it a `fetch` is refused and anything else dropped.
    let counted = !matches!(envelope.message, ModuleMessage::Pull(_));
    if counted && !host.allowance.admit(at) {
        return match envelope.message {
            ModuleMessage::Fetch(_) => Then::Refuse(
                envelope.id,
                Refusal::TooMany,
                "this frame sent more messages than a second allows",
            ),
            _ => Then::Nothing,
        };
    }

    // `ready` is the one message whose major is judged rather than filtered:
    // naming the wrong one is how a module says it does not speak this bridge.
    if let ModuleMessage::Ready(ready) = &envelope.message {
        let before = host.liveness.state();
        host.liveness.ready(envelope.bridge, host.mount.bridge, at);
        if let Some(kit) = &ready.kit {
            leptos::logging::log!(
                "module {}/{} is ready, built with {kit}",
                host.mount.platform,
                host.mount.panel
            );
        }
        let after = host.liveness.state();
        return match after {
            _ if after == before => Then::Nothing,
            PanelState::Ready => Then::Ready(host.view()),
            _ => Then::Publish(host.view()),
        };
    }
    if envelope.bridge[0] != host.mount.bridge {
        return Then::Nothing;
    }

    // A module that has not said `ready` has nothing to ask the surface for
    // yet; only its answers to the page's own questions count.
    let ready = matches!(host.liveness.state(), PanelState::Ready | PanelState::Stale);

    match envelope.message {
        ModuleMessage::Heartbeat(Heartbeat { n }) => {
            let before = host.liveness.state();
            host.liveness.echoed(n);
            let after = host.liveness.state();
            if after == before {
                Then::Nothing
            } else {
                Then::Publish(host.view())
            }
        }
        ModuleMessage::Fetch(fetch) => admit_fetch(host, envelope.id, fetch, room),
        // A stream's credit and its end, like a `fetch`, need no `ready`: they
        // are about a request the page already let the module make.
        ModuleMessage::Pull(pull) => Then::Pull(host.serial, pull),
        ModuleMessage::Cancel(cancel) => Then::Cancel(host.serial, cancel.re),
        ModuleMessage::State(state) => {
            // Only an answer to `suspend`; anything else is a module keeping
            // state the page never offered to hold.
            if host.suspending.is_none() {
                return Then::Nothing;
            }
            let kept = state
                .blob
                .filter(|blob| bridge::keeps_state(blob.len(), host.mount.limits.state_bytes));
            Then::Keep(kept)
        }
        _ if !ready => Then::Nothing,
        ModuleMessage::SetParam(set) => Then::SetParam(set.id, set.values),
        ModuleMessage::SetRange(range) => Then::Ask(Asked::Intent {
            instance: host.mount.instance.clone(),
            intent: hlin_view::Intent::Range {
                from_millis: range.from_millis,
                to_millis: range.to_millis,
            },
        }),
        ModuleMessage::Navigate(navigate) => Then::Ask(Asked::Navigate {
            instance: host.mount.instance.clone(),
            to: navigate.to,
        }),
        ModuleMessage::Changed(change) => Then::Changed(host.mount.platform.clone(), change),
        ModuleMessage::Notice(notice) => {
            let shown = bridge::notice_text(&notice.text).map(|text| ModuleNotice {
                level: notice.level,
                text,
            });
            if shown == host.notice {
                return Then::Nothing;
            }
            host.notice = shown;
            Then::Publish(host.view())
        }
        _ => Then::Nothing,
    }
}

fn admit_fetch(host: &mut Host, re: String, fetch: hlin_bridge::Fetch, room: StreamRoom) -> Then {
    if fetch.method == Method::Other {
        return Then::Refuse(
            re,
            Refusal::Method,
            "the bridge carries only HTTP's six methods",
        );
    }
    // A stream is a read only: a write's answer is a decision, not a feed.
    if fetch.stream && !matches!(fetch.method, Method::Get | Method::Head) {
        return Then::Refuse(re, Refusal::Method, "only a read may be streamed");
    }
    let size = fetch.body.as_ref().map_or(0, Vec::len) as u64;
    if size > host.mount.limits.request_bytes {
        return Then::Refuse(re, Refusal::TooLarge, "the request body is over its limit");
    }
    let address = match bridge::proxy_address(&host.mount.platform, &fetch.path, &fetch.query) {
        Ok(address) => address,
        Err(refusal) => {
            return Then::Refuse(
                re,
                refusal,
                "the path is not one this platform's module may ask for",
            );
        }
    };
    if fetch.stream
        && !bridge::admits_stream(
            room.in_frame,
            host.mount.limits.streams,
            room.on_page,
            room.page_cap,
        )
    {
        let reason = if room.on_page >= room.page_cap {
            "this page has as many streams open as it can hold"
        } else {
            "this frame has as many streams open as it may"
        };
        return Then::Refuse(re, Refusal::TooMany, reason);
    }
    if !host.allowance.begin_fetch() {
        return Then::Refuse(
            re,
            Refusal::TooMany,
            "this frame has too many requests in flight",
        );
    }
    Then::Carry(Carriage {
        serial: host.serial,
        re,
        address,
        fetch,
    })
}

/// A refusal the page makes itself, shaped as the shell would have made it.
fn refused(refusal: Refusal, reason: &str) -> Response {
    let code = serde_json::to_value(refusal).unwrap_or_default();
    let body = serde_json::json!({ "refusal": code, "reason": reason }).to_string();
    Response {
        status: bridge::refusal_status(refusal),
        headers: BTreeMap::from([("content-type".to_string(), "application/json".to_string())]),
        body: Some(body.into_bytes()),
        refusal: Some(refusal),
        streaming: false,
    }
}

/// Make a module's request to `/p/`, as the page, and answer it.
///
/// Same-origin, with the page's session: the shell checks it came from the
/// page (`Sec-Fetch-Site`, `Origin`), mints the viewer's identity, and calls
/// the platform. Never retried here; a module retries a read if it wants to,
/// and a write only when a person asks.
async fn carry(instance: String, carriage: Carriage) {
    let Carriage {
        serial,
        re,
        address,
        mut fetch,
    } = carriage;

    let answer = match request_for(&instance, &address, &mut fetch, None) {
        Ok(request) => match request.send().await {
            Ok(response) => whole(response).await,
            Err(_) => refused(Refusal::Unreachable, "the shell could not be reached"),
        },
        Err(_) => refused(Refusal::Unreachable, "the request could not be made"),
    };

    let current = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let host = page
            .hosts
            .get_mut(&instance)
            .filter(|host| host.serial == serial)?;
        host.allowance.end_fetch();
        Some(())
    });
    if current.is_some() {
        post(&instance, Some(re), ShellMessage::Response(answer));
    }
}

/// The request to `/p/` for a module's `fetch`: its method, the headers a
/// module may send, its key, its body, and for a stream the header asking for
/// one and the signal that aborts it.
fn request_for(
    instance: &str,
    address: &str,
    fetch: &mut hlin_bridge::Fetch,
    abort: Option<&web_sys::AbortSignal>,
) -> Result<gloo_net::http::Request, gloo_net::Error> {
    let method = match fetch.method {
        Method::Get => gloo_net::http::Method::GET,
        Method::Head => gloo_net::http::Method::HEAD,
        Method::Post => gloo_net::http::Method::POST,
        Method::Put => gloo_net::http::Method::PUT,
        Method::Patch => gloo_net::http::Method::PATCH,
        Method::Delete | Method::Other => gloo_net::http::Method::DELETE,
    };
    let mut builder = gloo_net::http::RequestBuilder::new(address)
        .method(method)
        .header("x-hlin-instance", instance);
    if fetch.stream {
        builder = builder.header(STREAM_HEADER, "1").abort_signal(abort);
    }
    for (name, value) in &fetch.headers {
        if hlin_bridge::is_allowed_request_header(name) {
            builder = builder.header(name, value);
        }
    }
    if let Some(key) = &fetch.idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    match fetch.body.take() {
        Some(bytes) => builder.body(js_sys::Uint8Array::from(bytes.as_slice())),
        None => builder.build(),
    }
}

/// An answer's status, and the headers a module may see, with the shell's
/// refusal named if it was one.
fn answered(
    response: &gloo_net::http::Response,
) -> (u16, BTreeMap<String, String>, Option<Refusal>) {
    let entries: Vec<(String, String)> = response.headers().entries().collect();
    let refusal = bridge::refusal_named(
        entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("x-hlin-refusal"))
            .map(|(_, value)| value.as_str()),
    );
    let headers = bridge::passed_back(
        entries
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    (response.status(), headers, refusal)
}

/// An answer read whole.
async fn whole(response: gloo_net::http::Response) -> Response {
    let (status, headers, refusal) = answered(&response);
    match response.binary().await {
        Ok(body) => Response {
            status,
            headers,
            body: Some(body),
            refusal,
            streaming: false,
        },
        Err(_) => refused(Refusal::Unreachable, "the answer stopped partway"),
    }
}

// -- Streams -------------------------------------------------------------------

/// Count a stream the page has admitted as open, with the signal that aborts
/// its request.
fn open_flow(instance: &str, carriage: &Carriage) -> Option<(u64, web_sys::AbortSignal)> {
    let abort = web_sys::AbortController::new().ok()?;
    let signal = abort.signal();
    let id = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.flow_serial += 1;
        let id = page.flow_serial;
        page.flows.insert(
            id,
            Flow {
                instance: instance.to_string(),
                serial: carriage.serial,
                re: carriage.re.clone(),
                outbox: bridge::Outbox::new(),
                abort,
                wake: None,
                ended: false,
            },
        );
        id
    });
    Some((id, signal))
}

/// End a stream: tell its module why, once, and stop its request.
///
/// Whoever ends a stream calls this — the reading task when the body is over,
/// a `cancel`, an unmount — and whichever comes first is the one the module
/// hears. Aborting the request is what makes the shell drop the platform's
/// connection. The flow stays counted as ended until its task has let go.
fn end_flow(id: u64, error: Option<EndError>) {
    let ended = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let flow = page.flows.get_mut(&id).filter(|flow| !flow.ended)?;
        flow.ended = true;
        flow.abort.abort();
        if let Some(wake) = flow.wake.take() {
            let _ = wake.send(());
        }
        let (instance, serial, re) = (flow.instance.clone(), flow.serial, flow.re.clone());
        let delivered = flow.outbox.delivered();
        let host = page
            .hosts
            .get(&instance)
            .filter(|host| host.serial == serial)?;
        Some((
            instance,
            re,
            format!("{}/{}", host.mount.platform, host.mount.panel),
            delivered,
        ))
    });
    let Some((instance, re, module, delivered)) = ended else {
        return;
    };
    // On the console, where an operator debugging a module looks, and where
    // a browser test can see an end the module itself did not live to.
    let why = error
        .and_then(|error| serde_json::to_value(error).ok())
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "finished".to_string());
    leptos::logging::log!("module {module}: stream {re} ended ({why}) after {delivered} bytes");
    post(&instance, None, ShellMessage::End(End { re, error }));
}

/// End every stream a frame holds, because its document is going
/// (*Streaming*: unmounting ends a frame's streams with `unmounted`).
///
/// Called before the frame leaves the document, so the module is told while
/// there is still someone to tell.
fn end_streams_of(instance: &str) {
    let open: Vec<u64> = PAGE.with(|page| {
        page.borrow()
            .flows
            .iter()
            .filter(|(_, flow)| flow.instance == instance && !flow.ended)
            .map(|(id, _)| *id)
            .collect()
    });
    for id in open {
        end_flow(id, Some(EndError::Unmounted));
    }
}

/// What a stream's reading task does next.
enum Step {
    /// Send the module this `chunk`.
    Send(u64, Vec<u8>),
    /// Everything read has been sent, and the shell said how it ended.
    End(Ended),
    /// Read more from the network.
    Read,
    /// Wait for credit.
    Wait(futures::channel::oneshot::Receiver<()>),
    /// Somebody else ended it.
    Gone,
}

/// Make a module's streamed request to `/p/`, and hand its body over as the
/// module asks for it (*Streaming*).
///
/// The page reads from the network only when it has sent everything it read
/// and the module has credit left, so it holds at most one read, and a
/// module that stops pulling stops the read. The browser then stops reading
/// the connection, the shell stops reading the platform, and the platform
/// stops being able to send: nothing buffers without bound anywhere.
async fn carry_stream(instance: String, carriage: Carriage, id: u64, signal: web_sys::AbortSignal) {
    let Carriage {
        serial,
        re,
        address,
        mut fetch,
    } = carriage;

    let sent = match request_for(&instance, &address, &mut fetch, Some(&signal)) {
        Ok(request) => request
            .send()
            .await
            .map_err(|_| "the shell could not be reached"),
        Err(_) => Err("the request could not be made"),
    };
    let gone = || PAGE.with(|page| page.borrow().flows.get(&id).is_none_or(|flow| flow.ended));

    match sent {
        Err(reason) => {
            if !gone() {
                post(
                    &instance,
                    Some(re),
                    ShellMessage::Response(refused(Refusal::Unreachable, reason)),
                );
            }
        }
        // Not streamed after all: the shell refused it before asking the
        // platform, and said so whole.
        Ok(response) if response.headers().get(STREAM_HEADER).is_none() => {
            let answer = whole(response).await;
            if !gone() {
                post(&instance, Some(re), ShellMessage::Response(answer));
            }
        }
        Ok(response) => {
            let (status, headers, _) = answered(&response);
            if !gone() {
                post(
                    &instance,
                    Some(re.clone()),
                    ShellMessage::Response(Response {
                        status,
                        headers,
                        body: None,
                        refusal: None,
                        streaming: true,
                    }),
                );
                follow(&instance, &re, id, response).await;
            }
        }
    }

    // Let go of it, and of its place in the frame's requests in flight.
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        if let Some(flow) = page.flows.remove(&id) {
            flow.abort.abort();
        }
        if let Some(host) = page
            .hosts
            .get_mut(&instance)
            .filter(|host| host.serial == serial)
        {
            host.allowance.end_fetch();
        }
    });
}

/// Read a streamed body and send it on as credit allows, until it ends.
async fn follow(instance: &str, re: &str, id: u64, response: gloo_net::http::Response) {
    // A `HEAD`, or a body the browser will not show: nothing to follow.
    let Some(reader) = response.body().and_then(|body| {
        body.get_reader()
            .dyn_into::<web_sys::ReadableStreamDefaultReader>()
            .ok()
    }) else {
        end_flow(id, None);
        return;
    };

    let mut decoder = Decoder::new();
    // How the shell said it ended, once it has.
    let mut ending: Option<Ended> = None;
    loop {
        let step = PAGE.with(|page| {
            let mut page = page.borrow_mut();
            let Some(flow) = page.flows.get_mut(&id).filter(|flow| !flow.ended) else {
                return Step::Gone;
            };
            if let Some((seq, body)) = flow.outbox.next_chunk() {
                return Step::Send(seq, body);
            }
            if let (true, Some(ending)) = (flow.outbox.is_empty(), ending) {
                return Step::End(ending);
            }
            if ending.is_none() && flow.outbox.wants_more() {
                return Step::Read;
            }
            let (wake, woken) = futures::channel::oneshot::channel();
            flow.wake = Some(wake);
            Step::Wait(woken)
        });

        match step {
            Step::Send(seq, body) => post(
                instance,
                None,
                ShellMessage::Chunk(Chunk {
                    re: re.to_string(),
                    seq,
                    body: Some(body),
                }),
            ),
            Step::End(ended) => {
                end_flow(id, bridge::end_error(ended));
                break;
            }
            Step::Read => match read(&reader).await {
                Some(Ok(piece)) => {
                    for frame in decoder.push(&piece) {
                        match frame {
                            Frame::Data(bytes) => PAGE.with(|page| {
                                if let Some(flow) = page.borrow_mut().flows.get_mut(&id) {
                                    flow.outbox.hold(bytes);
                                }
                            }),
                            Frame::End(why) => ending = Some(why),
                        }
                    }
                }
                // Over without the shell saying so, or broken off: cut
                // somewhere between. What was read still goes first.
                Some(Err(())) | None => ending = Some(ending.unwrap_or(Ended::Unreachable)),
            },
            Step::Wait(woken) => {
                let _ = woken.await;
            }
            Step::Gone => break,
        }
    }
    let _ = reader.cancel();
}

/// The next piece of a body: `None` at its end, an error if it broke off or
/// was aborted.
async fn read(reader: &web_sys::ReadableStreamDefaultReader) -> Option<Result<Vec<u8>, ()>> {
    let Ok(result) = wasm_bindgen_futures::JsFuture::from(reader.read()).await else {
        return Some(Err(()));
    };
    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
        .ok()
        .and_then(|done| done.as_bool())
        .unwrap_or(true);
    if done {
        return None;
    }
    let value = js_sys::Reflect::get(&result, &JsValue::from_str("value")).ok()?;
    Some(Ok(js_sys::Uint8Array::new(&value).to_vec()))
}

// -- The view ------------------------------------------------------------------

/// Where a panel's module is drawn: an empty box the page puts a frame in.
///
/// Rendered only while the module is wanted; when the view drops it, the frame
/// is unmounted, registry first.
#[component]
pub fn ModuleFrame(
    /// What to mount.
    mount: Mount,
    /// The panel's title and its platform, for assistive technology.
    title: Signal<String>,
) -> impl IntoView {
    let holder: NodeRef<leptos::html::Div> = NodeRef::new();
    let instance = mount.instance.clone();

    Effect::new(move |_| {
        let title = title.get();
        if let Some(element) = holder.get() {
            host(element.into(), mount.clone(), title);
        }
    });
    on_cleanup(move || {
        PAGE.with(|page| page.borrow_mut().restored.remove(&instance));
        unmount(&instance);
    });

    view! { <div class="module-frame" node_ref=holder></div> }
}
