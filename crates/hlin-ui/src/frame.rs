//! Hosting a platform's module in a sandboxed frame: the parent end of the
//! bridge (specification HLIN-S-0007).
//!
//! The DOM half of [`crate::bridge`], and deliberately only wiring: every
//! decision — which frame sent a message, what state a quiet module is in,
//! whether a request is one too many, where a `fetch` may go — is made there
//! and tested there. What is here is the part that cannot be tested without a
//! browser: creating the frame, one `message` listener for the whole page, one
//! clock, the observers that say where a panel is, and the requests to `/p/`.
//!
//! Frames are created imperatively rather than through the view, for one
//! reason the specification insists on: a frame leaving the surface is removed
//! from the registry *before* it is removed from the document, so a message
//! already in flight from it is dropped. Only code that owns both steps can
//! promise their order.
//!
//! The page's state lives in one thread-local, because the browser is single
//! threaded and every entry point here is a browser callback. Nothing that
//! re-enters it (posting a message, publishing a state to the view) is done
//! while it is borrowed.

use std::cell::RefCell;
use std::collections::BTreeMap;

use hlin_bridge::{
    Context, Envelope, Heartbeat, IdMint, Init, Limits, Method, ModuleMessage, Refusal, Response,
    Selections, ShellMessage, Theme, TimeRange, Viewer, Visibility,
};
use hlin_stream::layout::ModuleLimits;
use hlin_view::{Cause, PanelState};
use leptos::prelude::*;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};

use crate::bridge::{self, Allowance, Due, Liveness, Registered, Registry};

/// What the surface draws for one module panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModuleView {
    /// Where the module is, from the bridge.
    pub state: PanelState,
    /// The module is unavailable for a reason about itself, and the panel
    /// declares data, so the shell draws that instead.
    pub fallen_back: bool,
}

impl Default for ModuleView {
    fn default() -> Self {
        Self {
            state: PanelState::Loading,
            fallen_back: false,
        }
    }
}

/// Everything needed to mount one panel's module, and nothing that changes
/// while it is mounted: a change to any of this is a different module, and
/// remounts it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    /// The platform whose module it is.
    pub platform: String,
    /// The panel key.
    pub panel: String,
    /// The panel instance.
    pub instance: String,
    /// The entry document, relative to the platform's base.
    pub entry: String,
    /// The bridge major the manifest declared.
    pub bridge: u32,
    /// Whether the panel declares data to fall back to.
    pub declares_data: bool,
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

/// One mounted module.
struct Host {
    mount: Mount,
    /// Which mounting this is, so an answer to a request made by a frame since
    /// torn down and remounted is not delivered to its successor.
    serial: u64,
    container: web_sys::Element,
    iframe: Option<web_sys::HtmlIFrameElement>,
    title: String,
    liveness: Liveness,
    allowance: Allowance,
    ids: IdMint,
    /// Whether the panel is in the viewport, as the observer last said.
    in_view: bool,
    observers: Vec<web_sys::IntersectionObserver>,
    /// Kept alive for as long as the observers and the frame can call them.
    _callbacks: Vec<Closure<dyn FnMut(js_sys::Array)>>,
    _onload: Option<Closure<dyn FnMut()>>,
}

/// What every module on this surface is told about where it is.
#[derive(Debug, Clone, Default)]
struct Surroundings {
    viewer: Option<String>,
    read_only: bool,
    time_range: Option<TimeRange>,
    generation: u64,
    params: BTreeMap<String, Selections>,
}

#[derive(Default)]
struct Page {
    registry: Registry<JsValue>,
    hosts: BTreeMap<String, Host>,
    published: Option<RwSignal<BTreeMap<String, ModuleView>>>,
    surroundings: Surroundings,
    serial: u64,
    started: bool,
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
        !std::mem::replace(&mut page.started, true)
    });
    if !first {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };

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

/// The surface's time range and each panel's parameters, for every `init`.
///
/// Only recorded here. Telling a mounted module that they changed is `context`,
/// which is the next piece of work (HLIN-T-0067); this is where it will hook in.
pub fn set_context(
    time_range: Option<TimeRange>,
    generation: u64,
    params: BTreeMap<String, Selections>,
) {
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        page.surroundings.time_range = time_range;
        page.surroundings.generation = generation;
        page.surroundings.params = params;
    });
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
            if intersecting(&entries) {
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

/// The panel came near the viewport: create its frame, once.
fn attach(instance: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };

    let prepared = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let host = page.hosts.get_mut(instance)?;
        if host.iframe.is_some() {
            return None;
        }
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

        let _ = host.container.append_child(&iframe);
        // The window exists once the frame is in the document, and it is the
        // same window after the frame navigates to its source, which is what
        // lets it be registered before anything in it can run.
        let window = iframe.content_window()?;
        let registered = Registered {
            platform: host.mount.platform.clone(),
            panel: host.mount.panel.clone(),
            instance: instance.to_string(),
            mounted_at: now(),
        };
        let entry = format!("/m/{}{}", host.mount.platform, host.mount.entry);
        let serial = host.serial;
        host.iframe = Some(iframe);
        page.registry.register(JsValue::from(window), registered);
        Some((entry, serial))
    });

    let Some((entry, serial)) = prepared else {
        return;
    };

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
                Some(settled(host.liveness.state(), host.mount.declares_data))
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
    let changed = PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let host = page.hosts.get_mut(instance)?;
        if let Some(in_view) = in_view {
            host.in_view = in_view;
        }
        let visible = host.in_view && document_visible();
        if visible == host.liveness.is_visible() {
            return None;
        }
        host.liveness.visible(visible, now());
        host.liveness.has_loaded().then_some(visible)
    });
    if let Some(visible) = changed {
        post(
            instance,
            None,
            ShellMessage::Visibility(Visibility { visible }),
        );
    }
}

/// Stop hosting a panel's module: out of the registry, then out of the
/// document. Called when its panel leaves the surface, and when the module is
/// given up on.
pub fn unmount(instance: &str) {
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
}

/// Mount a module again after it was given up on: the only way `unavailable`
/// recovers (*Panel states*).
pub fn retry(instance: &str) {
    if let Some(published) = PAGE.with(|page| page.borrow().published) {
        published.update(|views| {
            views.remove(instance);
        });
    }
}

// -- The clock ---------------------------------------------------------------

fn tick() {
    let at = now();
    let mut due = Vec::new();
    let mut changed = Vec::new();

    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        for (id, host) in page.hosts.iter_mut() {
            let before = host.liveness.state();
            if let Some(owed) = host.liveness.tick(at) {
                due.push((id.clone(), owed));
            }
            let after = host.liveness.state();
            if after != before {
                changed.push((id.clone(), settled(after, host.mount.declares_data)));
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
}

fn init_for(instance: &str) -> Option<Init> {
    PAGE.with(|page| {
        let page = page.borrow();
        let host = page.hosts.get(instance)?;
        let around = &page.surroundings;
        let limits = host.mount.limits;
        Some(Init {
            platform: host.mount.platform.clone(),
            panel: host.mount.panel.clone(),
            instance: instance.to_string(),
            page: false,
            context: Context {
                time_range: around.time_range,
                params: around.params.get(instance).cloned().unwrap_or_default(),
                generation: around.generation,
            },
            // The scheme and tokens are sent, and kept current, with `theme`
            // (HLIN-T-0067). Until then a module draws its own defaults.
            theme: Theme::default(),
            viewer: Viewer {
                name: around.viewer.clone(),
            },
            read_only: around.read_only,
            limits: Limits {
                request_bytes: limits.request_bytes,
                response_bytes: limits.response_bytes,
                fetches_in_flight: u64::from(limits.fetches_in_flight),
                streams: u64::from(limits.streams),
            },
            restored: None,
        })
    })
}

// -- Publishing --------------------------------------------------------------

fn settled(state: PanelState, declares_data: bool) -> ModuleView {
    ModuleView {
        state,
        fallen_back: bridge::falls_back(state, declares_data),
    }
}

fn publish(instance: &str, view: ModuleView) {
    let Some(published) = PAGE.with(|page| page.borrow().published) else {
        return;
    };
    let current = published.with_untracked(|views| views.get(instance).copied());
    if current != Some(view) {
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
        let id = instance.to_string();
        leptos::task::spawn_local(async move {
            unmount(&id);
            publish(&id, view);
        });
        return;
    }
    publish(instance, view);
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
    Refuse(String, Refusal, &'static str),
    Carry(Carriage),
}

/// A `fetch` the page will make.
struct Carriage {
    serial: u64,
    re: String,
    address: String,
    fetch: hlin_bridge::Fetch,
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
        let host = page.hosts.get_mut(&instance)?;
        Some((instance, handle(host, envelope, at)))
    });

    let Some((instance, then)) = handled else {
        return;
    };
    match then {
        Then::Nothing => {}
        Then::Publish(view) => conclude(&instance, view),
        Then::Refuse(re, refusal, reason) => post(
            &instance,
            Some(re),
            ShellMessage::Response(refused(refusal, reason)),
        ),
        Then::Carry(carriage) => {
            leptos::task::spawn_local(carry(instance, carriage));
        }
    }
}

fn handle(host: &mut Host, envelope: Envelope<ModuleMessage>, at: f64) -> Then {
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
        return if after == before {
            Then::Nothing
        } else {
            Then::Publish(settled(after, host.mount.declares_data))
        };
    }
    if envelope.bridge[0] != host.mount.bridge {
        return Then::Nothing;
    }

    match envelope.message {
        ModuleMessage::Heartbeat(Heartbeat { n }) => {
            let before = host.liveness.state();
            host.liveness.echoed(n);
            let after = host.liveness.state();
            if after == before {
                Then::Nothing
            } else {
                Then::Publish(settled(after, host.mount.declares_data))
            }
        }
        ModuleMessage::Fetch(fetch) => admit_fetch(host, envelope.id, fetch),
        // `context`'s counterparts, `changed`, `notice`, `navigate` and the
        // stream's credit are the next pieces of work (HLIN-T-0067, HLIN-T-0068).
        _ => Then::Nothing,
    }
}

fn admit_fetch(host: &mut Host, re: String, fetch: hlin_bridge::Fetch) -> Then {
    if fetch.method == Method::Other {
        return Then::Refuse(
            re,
            Refusal::Method,
            "the bridge carries only HTTP's six methods",
        );
    }
    if fetch.stream {
        // A streamed read is HLIN-T-0068's; until it lands a module asking for
        // one is told the method is not carried, which is what it would hear
        // for `stream` on a write.
        return Then::Refuse(
            re,
            Refusal::Method,
            "streamed responses are not carried yet",
        );
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

    let method = match fetch.method {
        Method::Get => gloo_net::http::Method::GET,
        Method::Head => gloo_net::http::Method::HEAD,
        Method::Post => gloo_net::http::Method::POST,
        Method::Put => gloo_net::http::Method::PUT,
        Method::Patch => gloo_net::http::Method::PATCH,
        Method::Delete | Method::Other => gloo_net::http::Method::DELETE,
    };
    let mut builder = gloo_net::http::RequestBuilder::new(&address)
        .method(method)
        .header("x-hlin-instance", &instance);
    for (name, value) in &fetch.headers {
        if hlin_bridge::is_allowed_request_header(name) {
            builder = builder.header(name, value);
        }
    }
    if let Some(key) = &fetch.idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    let request = match fetch.body.take() {
        Some(bytes) => builder.body(js_sys::Uint8Array::from(bytes.as_slice())),
        None => builder.build(),
    };

    let answer = match request {
        Ok(request) => match request.send().await {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers();
                let entries: Vec<(String, String)> = headers.entries().collect();
                let refusal = bridge::refusal_named(
                    entries
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("x-hlin-refusal"))
                        .map(|(_, value)| value.as_str()),
                );
                match response.binary().await {
                    Ok(body) => Response {
                        status,
                        headers: bridge::passed_back(
                            entries
                                .iter()
                                .map(|(name, value)| (name.as_str(), value.as_str())),
                        ),
                        body: Some(body),
                        refusal,
                        streaming: false,
                    },
                    Err(_) => refused(Refusal::Unreachable, "the answer stopped partway"),
                }
            }
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
    on_cleanup(move || unmount(&instance));

    view! { <div class="module-frame" node_ref=holder></div> }
}
