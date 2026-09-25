#![warn(missing_docs)]

//! The SDK for a Leptos module hosted by Hlin.
//!
//! A platform ships its UI as a module, and the shell runs each one in a
//! sandboxed frame with no network, no cookies and no view of the page around
//! it (decision HLIN-A-0014). Everything a module needs it asks the shell's
//! page for over `postMessage`, in the protocol specification HLIN-S-0007
//! defines and `hlin-bridge` encodes. This crate speaks that protocol so a
//! module does not have to: it answers the handshake and the heartbeat, turns
//! the shell's context and theme into Leptos signals, and gives a module
//! `fetch` to its own platform as an ordinary async call. A module built with
//! it needs no bridge code of its own (NFR-1.2).
//!
//! # A minimal module
//!
//! ```no_run
//! use hlin_module::{Reply, Request, connect};
//! use leptos::prelude::*;
//!
//! fn main() {
//!     // `wasm_bindgen_futures`, not `leptos::task::spawn_local`: nothing is
//!     // mounted yet, so Leptos has no executor to spawn onto and would panic.
//!     wasm_bindgen_futures::spawn_local(async {
//!         // Waits for the shell's `init`.
//!         let module = connect().await;
//!         let context = module.context();
//!
//!         let fetcher = module.clone();
//!         let items = LocalResource::new(move || {
//!             // Refetch whenever the time range or a parameter changes.
//!             let list = context.get().params.get("list").cloned().unwrap_or_default();
//!             let module = fetcher.clone();
//!             async move {
//!                 let request = Request::get("/api/items").query(format!("list={}", list.join(",")));
//!                 match module.fetch(request).await {
//!                     Ok(Reply::Answered(answer)) if answer.is_success() => answer.text(),
//!                     Ok(Reply::Answered(answer)) => format!("The platform said {}", answer.status),
//!                     Ok(Reply::Refused(refused)) => format!("Hlin refused: {:?}", refused.refusal),
//!                     Err(error) => error.to_string(),
//!                 }
//!             }
//!         });
//!
//!         mount_to_body(move || view! { <pre>{move || items.get()}</pre> });
//!         // Tell the shell we can draw, and which kit we were built with.
//!         module.ready(Some("aurora@0.2.1"));
//!     });
//! }
//! ```
//!
//! # Writes
//!
//! A write carries an idempotency key, and the shell never retries it. The SDK
//! mints a key per [`Attempt`]; retrying the attempt, because a person pressed
//! *Retry*, sends the same key, so a platform that already did it can say so
//! rather than doing it twice.
//!
//! ```no_run
//! # async fn toggle(module: hlin_module::Module) -> Result<(), Box<dyn std::error::Error>> {
//! use hlin_module::Request;
//!
//! let attempt = module.attempt(Request::post("/api/lists/team/items/i1/toggle").json(&true)?);
//! let reply = attempt.send().await?;
//! if reply.answered().is_none_or(|answer| !answer.is_success()) {
//!     // Later, when the person asks:
//!     attempt.retry().await?;
//! }
//! module.changed("items", Default::default());
//! # Ok(())
//! # }
//! ```
//!
//! # Testing without a browser
//!
//! Everything that touches the browser is behind [`Host`]. [`connect`] uses the
//! frame's own window; a test gives [`Module::new`] a host that records what
//! the module sends and feeds [`Module::receive`] what the shell would say.

mod key;
mod request;
mod stream;
mod web;

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;

use futures::channel::{mpsc, oneshot};
use hlin_bridge::{
    Cancel, ChangeSource, Context, End, Envelope, Fetch, Heartbeat, IdMint, Init, Limits, MAJOR,
    ModuleChanged, ModuleMessage, NOTICE_MAX_CHARS, Navigate, Notice, NoticeLevel, Pull, Ready,
    Response, Selections, SetParam, ShellMessage, State, Target, Theme, TimeRange, Viewer,
};
use leptos::prelude::*;
use send_wrapper::SendWrapper;

pub use hlin_bridge;
pub use request::{Answer, BridgeError, Refused, Reply, Request};
pub use stream::{BodyReader, CREDIT_WINDOW, StreamError, StreamReply};
pub use web::{WebHost, connect};

/// Everything the SDK needs from the world outside the module's own logic.
///
/// A trait so the protocol can be tested natively: [`WebHost`] is the real
/// one, and a test supplies one that records.
pub trait Host {
    /// Sends a message to the shell's page.
    fn post(&self, envelope: Envelope<ModuleMessage>);
    /// Applies the shell's theme to the frame's document.
    fn apply_theme(&self, theme: &Theme);
    /// Fills `buffer` with random bytes, for idempotency keys.
    fn random_bytes(&self, buffer: &mut [u8]);
    /// Milliseconds since the Unix epoch, for idempotency keys.
    fn now_millis(&self) -> u64;
}

/// Who this module is, from `init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The platform whose module this is.
    pub platform: String,
    /// The panel key, or the page path for a page.
    pub panel: String,
    /// This mounted instance.
    pub instance: String,
    /// Open as a full-width page rather than a panel.
    pub page: bool,
}

/// A `changed` the shell relayed, numbered so that two identical changes in a
/// row still wake whatever watches for them.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Counts up from 1 for the life of the module.
    pub seq: u64,
    /// The panel it concerns.
    pub panel: String,
    /// The selections it was made under.
    pub selections: Selections,
    /// The platform's event stream, or another of its modules.
    pub from: ChangeSource,
}

/// Where the handshake is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the shell's `init`.
    AwaitingInit,
    /// `init` has arrived; the module has not said `ready`.
    Initialised,
    /// The module has said `ready`.
    Ready,
}

/// The module's end of the bridge. Cheap to clone; every clone is the same
/// bridge.
#[derive(Clone)]
pub struct Module {
    // `Send + Sync` so it can go in Leptos context and view closures, which
    // ask for both. A module has one thread; this panics if that ever stops
    // being true, rather than racing.
    inner: SendWrapper<Rc<Inner>>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}

struct Inner {
    host: Box<dyn Host>,
    state: RefCell<Shared>,
    context: ArcRwSignal<Context>,
    theme: ArcRwSignal<Theme>,
    visible: ArcRwSignal<bool>,
    viewer: ArcRwSignal<Viewer>,
    read_only: ArcRwSignal<bool>,
    limits: ArcRwSignal<Limits>,
    changes: ArcRwSignal<Option<Change>>,
}

type SuspendHook = Rc<dyn Fn() -> Option<Vec<u8>>>;

struct Shared {
    ids: IdMint,
    phase: Phase,
    identity: Option<Identity>,
    restored: Option<Vec<u8>>,
    ready_wanted: Option<Ready>,
    init_waiters: Vec<oneshot::Sender<()>>,
    pending: HashMap<String, Pending>,
    streams: HashMap<String, mpsc::UnboundedSender<stream::StreamEvent>>,
    suspend: Option<SuspendHook>,
    changes_seen: u64,
}

struct Pending {
    answer: oneshot::Sender<(
        Response,
        Option<mpsc::UnboundedReceiver<stream::StreamEvent>>,
    )>,
    stream: bool,
}

impl Module {
    /// A module on the given host, waiting for `init`. In a browser, use
    /// [`connect`], which also listens to the frame's parent.
    pub fn new(host: impl Host + 'static) -> Self {
        Self {
            inner: SendWrapper::new(Rc::new(Inner {
                host: Box::new(host),
                state: RefCell::new(Shared {
                    ids: IdMint::module(),
                    phase: Phase::AwaitingInit,
                    identity: None,
                    restored: None,
                    ready_wanted: None,
                    init_waiters: Vec::new(),
                    pending: HashMap::new(),
                    streams: HashMap::new(),
                    suspend: None,
                    changes_seen: 0,
                }),
                context: ArcRwSignal::new(Context::default()),
                theme: ArcRwSignal::new(Theme::default()),
                visible: ArcRwSignal::new(true),
                viewer: ArcRwSignal::new(Viewer::default()),
                read_only: ArcRwSignal::new(false),
                limits: ArcRwSignal::new(Limits::default()),
                changes: ArcRwSignal::new(None),
            })),
        }
    }

    // --- The handshake -----------------------------------------------------

    /// Where the handshake is.
    pub fn phase(&self) -> Phase {
        self.inner.state.borrow().phase
    }

    /// Resolves once `init` has arrived: at once, if it already has.
    pub fn initialised(&self) -> impl Future<Output = ()> + 'static {
        let waiting = {
            let mut state = self.inner.state.borrow_mut();
            if state.phase == Phase::AwaitingInit {
                let (sender, receiver) = oneshot::channel();
                state.init_waiters.push(sender);
                Some(receiver)
            } else {
                None
            }
        };
        async move {
            if let Some(receiver) = waiting {
                let _ = receiver.await;
            }
        }
    }

    /// Tells the shell the module can draw, optionally naming the shared kit
    /// it was built against (`"aurora@0.2.1"`). Sent once; a second call does
    /// nothing. Called before `init` has arrived, it is sent when it does.
    pub fn ready(&self, kit: Option<&str>) {
        let ready = Ready {
            kit: kit.map(str::to_string),
        };
        let send = {
            let mut state = self.inner.state.borrow_mut();
            match state.phase {
                Phase::AwaitingInit => {
                    state.ready_wanted.get_or_insert(ready);
                    None
                }
                Phase::Initialised => {
                    state.phase = Phase::Ready;
                    Some(ready)
                }
                Phase::Ready => None,
            }
        };
        if let Some(ready) = send {
            self.send(ModuleMessage::Ready(ready));
        }
    }

    /// Who this module is. `None` until `init` has arrived, which [`connect`]
    /// waits for.
    pub fn identity(&self) -> Option<Identity> {
        self.inner.state.borrow().identity.clone()
    }

    /// What the module handed back at its last `suspend` on this page, if
    /// anything: the way to pick up where it was before it was unmounted.
    pub fn restored(&self) -> Option<Vec<u8>> {
        self.inner.state.borrow().restored.clone()
    }

    /// What to keep when the shell unmounts this frame to stay within its
    /// budget. Called on `suspend`; the bytes come back as [`Self::restored`]
    /// when the frame is mounted again on the same page. Return `None` to keep
    /// nothing. Without a hook, `suspend` is answered at once with nothing
    /// kept. Keep it small: the page keeps at most `state_bytes` (64 KiB by
    /// default) and drops anything larger. A later call replaces the hook.
    pub fn on_suspend(&self, hook: impl Fn() -> Option<Vec<u8>> + 'static) {
        self.inner.state.borrow_mut().suspend = Some(Rc::new(hook));
    }

    // --- What the shell says, as signals ------------------------------------

    /// The surface's time range and the panel's parameters.
    pub fn context(&self) -> Signal<Context> {
        self.inner.context.read_only().into()
    }

    /// The colour scheme and tokens. Already applied to the document as CSS
    /// custom properties; this is for anything drawn outside CSS.
    pub fn theme(&self) -> Signal<Theme> {
        self.inner.theme.read_only().into()
    }

    /// Whether the panel is in view and the tab showing. Stop timers and
    /// animation when it is not.
    pub fn visible(&self) -> Signal<bool> {
        self.inner.visible.read_only().into()
    }

    /// Who is looking, for display only.
    pub fn viewer(&self) -> Signal<Viewer> {
        self.inner.viewer.read_only().into()
    }

    /// Writes will be refused; stop offering them.
    pub fn read_only(&self) -> Signal<bool> {
        self.inner.read_only.read_only().into()
    }

    /// The limits in force for this platform.
    pub fn limits(&self) -> Signal<Limits> {
        self.inner.limits.read_only().into()
    }

    /// The last `changed` the shell relayed: refetch if it concerns you.
    pub fn changes(&self) -> Signal<Option<Change>> {
        self.inner.changes.read_only().into()
    }

    // --- Requests ------------------------------------------------------------

    /// Starts an attempt at a request, minting its idempotency key if it is a
    /// write. Send it with [`Attempt::send`]; retry with [`Attempt::retry`].
    pub fn attempt(&self, request: Request) -> Attempt {
        let key = request.method.is_write().then(|| self.mint_key());
        Attempt {
            module: self.clone(),
            request,
            key,
        }
    }

    /// Sends a request once. For a write that a person may want to retry, keep
    /// the [`Attempt`] instead.
    pub async fn fetch(&self, request: Request) -> Result<Reply, BridgeError> {
        self.attempt(request).send().await
    }

    /// Sends a read and takes its body as it arrives. A write cannot be
    /// streamed.
    pub async fn fetch_stream(&self, request: Request) -> Result<StreamReply, BridgeError> {
        if !request.method.is_read() {
            return Err(BridgeError::StreamOnWrite);
        }
        let (id, answer) = self.send_fetch(&request, None, true);
        let (response, events) = answer.await.map_err(|_| BridgeError::Gone)?;
        Ok(match (events, response.refusal) {
            (Some(events), None) => StreamReply::Streaming {
                status: response.status,
                headers: response.headers,
                body: BodyReader::open(self.clone(), id, events),
            },
            _ => match Reply::from_response(response) {
                Reply::Answered(answer) => StreamReply::Answered(answer),
                Reply::Refused(refused) => StreamReply::Refused(refused),
            },
        })
    }

    fn mint_key(&self) -> String {
        let mut random = [0_u8; 10];
        self.inner.host.random_bytes(&mut random);
        key::ulid(self.inner.host.now_millis(), random)
    }

    #[allow(clippy::type_complexity)]
    fn send_fetch(
        &self,
        request: &Request,
        key: Option<String>,
        stream: bool,
    ) -> (
        String,
        oneshot::Receiver<(
            Response,
            Option<mpsc::UnboundedReceiver<stream::StreamEvent>>,
        )>,
    ) {
        let (sender, receiver) = oneshot::channel();
        let id = {
            let mut state = self.inner.state.borrow_mut();
            let id = state.ids.mint();
            state.pending.insert(
                id.clone(),
                Pending {
                    answer: sender,
                    stream,
                },
            );
            id
        };
        let fetch = Fetch {
            method: request.method,
            path: request.path.clone(),
            query: request.query.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
            idempotency_key: key,
            stream,
        };
        self.inner
            .host
            .post(Envelope::new(id.clone(), ModuleMessage::Fetch(fetch)));
        (id, receiver)
    }

    pub(crate) fn pull(&self, re: &str, bytes: u64) {
        self.send(ModuleMessage::Pull(Pull {
            re: re.to_string(),
            bytes,
        }));
    }

    pub(crate) fn cancel_stream(&self, re: &str) {
        let open = self.inner.state.borrow_mut().streams.remove(re).is_some();
        if open {
            self.send(ModuleMessage::Cancel(Cancel { re: re.to_string() }));
        }
    }

    // --- What a module tells the shell ----------------------------------------

    /// Selects values for a parameter the panel declares, as the chrome's own
    /// control would.
    pub fn set_param(&self, id: impl Into<String>, values: Vec<String>) {
        self.send(ModuleMessage::SetParam(SetParam {
            id: id.into(),
            values,
        }));
    }

    /// Sets the surface's time range.
    pub fn set_range(&self, from_millis: i64, to_millis: i64) {
        self.send(ModuleMessage::SetRange(TimeRange {
            from_millis,
            to_millis,
        }));
    }

    /// Opens a page or panel in the shell, of this platform or another.
    pub fn navigate(&self, to: Target) {
        self.send(ModuleMessage::Navigate(Navigate { to }));
    }

    /// Says the module wrote something, so the platform's other mounted
    /// modules can refetch. The shell never infers this from a write.
    pub fn changed(&self, panel: impl Into<String>, selections: Selections) {
        self.send(ModuleMessage::Changed(ModuleChanged {
            panel: panel.into(),
            selections,
        }));
    }

    /// A short message in the panel's frame, labelled as the platform's.
    /// Shortened to 140 characters here, because the shell would cut it
    /// anyway and a module should see what will be shown.
    pub fn notice(&self, level: NoticeLevel, text: &str) {
        self.send(ModuleMessage::Notice(Notice {
            level,
            text: text.chars().take(NOTICE_MAX_CHARS).collect(),
        }));
    }

    fn send(&self, message: ModuleMessage) {
        let id = self.inner.state.borrow_mut().ids.mint();
        self.inner.host.post(Envelope::new(id, message));
    }

    fn reply(&self, re: String, message: ModuleMessage) {
        let id = self.inner.state.borrow_mut().ids.mint();
        self.inner.host.post(Envelope::reply(id, re, message));
    }

    // --- What the shell says --------------------------------------------------

    /// Handles one message from the shell's page. [`connect`] calls this for
    /// every message from the frame's parent; a test calls it directly.
    ///
    /// A message from a major this SDK does not speak is ignored. Before
    /// `init`, only the heartbeat and answers to requests are acted on:
    /// context, theme and the rest mean nothing until the module knows who it
    /// is, and `init` carries all of them anyway.
    pub fn receive(&self, envelope: Envelope<ShellMessage>) {
        if envelope.bridge[0] != MAJOR {
            return;
        }
        let Envelope {
            id, re, message, ..
        } = envelope;
        let initialised = self.phase() != Phase::AwaitingInit;
        match message {
            ShellMessage::Init(init) if !initialised => self.on_init(init),
            ShellMessage::Init(_) => {}
            ShellMessage::Heartbeat(Heartbeat { n }) => {
                self.reply(id, ModuleMessage::Heartbeat(Heartbeat { n }));
            }
            ShellMessage::Response(response) => {
                if let Some(re) = re {
                    self.on_response(re, response);
                }
            }
            ShellMessage::Chunk(chunk) => {
                let state = self.inner.state.borrow();
                if let Some(events) = state.streams.get(&chunk.re) {
                    let _ = events
                        .unbounded_send(stream::StreamEvent::Chunk(chunk.body.unwrap_or_default()));
                }
            }
            ShellMessage::End(End { re, error }) => {
                let events = self.inner.state.borrow_mut().streams.remove(&re);
                if let Some(events) = events {
                    let _ = events.unbounded_send(stream::StreamEvent::End(error));
                }
            }
            _ if !initialised => {}
            ShellMessage::Context(context) => self.inner.context.set(context),
            ShellMessage::Theme(theme) => {
                self.inner.host.apply_theme(&theme);
                self.inner.theme.set(theme);
            }
            ShellMessage::Visibility(visibility) => self.inner.visible.set(visibility.visible),
            ShellMessage::Changed(changed) => {
                let seq = {
                    let mut state = self.inner.state.borrow_mut();
                    state.changes_seen += 1;
                    state.changes_seen
                };
                self.inner.changes.set(Some(Change {
                    seq,
                    panel: changed.panel,
                    selections: changed.selections,
                    from: changed.from,
                }));
            }
            // Answered at once, whatever there is to keep: the frame is in
            // the document, and counted against the page's budget, until the
            // page has its answer or gives up waiting for one. A module that
            // keeps nothing says so rather than make the page wait out the
            // deadline.
            ShellMessage::Suspend(_) => {
                let hook = self.inner.state.borrow().suspend.clone();
                let blob = hook.and_then(|hook| hook());
                self.reply(id, ModuleMessage::State(State { blob }));
            }
        }
    }

    fn on_init(&self, init: Init) {
        let Init {
            platform,
            panel,
            instance,
            page,
            context,
            theme,
            viewer,
            read_only,
            limits,
            restored,
        } = init;
        self.inner.host.apply_theme(&theme);
        self.inner.context.set(context);
        self.inner.theme.set(theme);
        self.inner.viewer.set(viewer);
        self.inner.read_only.set(read_only);
        self.inner.limits.set(limits);
        let (waiters, ready) = {
            let mut state = self.inner.state.borrow_mut();
            state.identity = Some(Identity {
                platform,
                panel,
                instance,
                page,
            });
            state.restored = restored;
            state.phase = Phase::Initialised;
            (
                std::mem::take(&mut state.init_waiters),
                state.ready_wanted.take(),
            )
        };
        for waiter in waiters {
            let _ = waiter.send(());
        }
        if let Some(ready) = ready {
            self.ready(ready.kit.as_deref());
        }
    }

    fn on_response(&self, re: String, response: Response) {
        let mut state = self.inner.state.borrow_mut();
        let Some(pending) = state.pending.remove(&re) else {
            return;
        };
        let events = if pending.stream && response.streaming && response.refusal.is_none() {
            let (sender, receiver) = mpsc::unbounded();
            state.streams.insert(re, sender);
            Some(receiver)
        } else {
            None
        };
        drop(state);
        let _ = pending.answer.send((response, events));
    }
}

/// One attempt at a request, holding its idempotency key.
///
/// A write's key is minted when the attempt is made and sent every time it is
/// sent, so [`Attempt::retry`] is the same attempt again, not a new one. A new
/// write is a new attempt, with a new key.
#[derive(Debug, Clone)]
pub struct Attempt {
    module: Module,
    request: Request,
    key: Option<String>,
}

impl Attempt {
    /// The idempotency key: `Some` for a write, `None` for a read.
    pub fn idempotency_key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// Sends the request.
    pub async fn send(&self) -> Result<Reply, BridgeError> {
        let (_, answer) = self
            .module
            .send_fetch(&self.request, self.key.clone(), false);
        let (response, _) = answer.await.map_err(|_| BridgeError::Gone)?;
        Ok(Reply::from_response(response))
    }

    /// Sends it again with the same key, because a person asked. The shell
    /// never retries a write, and the SDK does only here.
    pub async fn retry(&self) -> Result<Reply, BridgeError> {
        self.send().await
    }
}

#[cfg(test)]
mod tests;
