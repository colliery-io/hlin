//! The protocol, natively: a host that records what the module sends, and the
//! shell's side played by hand.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use hlin_bridge::{
    ChangeSource, Chunk, Context, End, EndError, Envelope, Heartbeat, Init, Limits, Method,
    ModuleMessage, NoticeLevel, Refusal, Response, Scheme, ShellChanged, ShellMessage, Suspend,
    Target, Theme, TimeRange, Viewer, Visibility,
};
use leptos::prelude::*;

use super::*;

#[derive(Clone, Default)]
struct Recorder {
    sent: Rc<RefCell<Vec<Envelope<ModuleMessage>>>>,
    themes: Rc<RefCell<Vec<Theme>>>,
    randomness: Rc<Cell<u8>>,
}

impl Recorder {
    fn take(&self) -> Vec<Envelope<ModuleMessage>> {
        std::mem::take(&mut *self.sent.borrow_mut())
    }

    fn only(&self) -> Envelope<ModuleMessage> {
        let mut sent = self.take();
        assert_eq!(sent.len(), 1, "expected one message, got {sent:?}");
        sent.remove(0)
    }
}

impl Host for Recorder {
    fn post(&self, envelope: Envelope<ModuleMessage>) {
        self.sent.borrow_mut().push(envelope);
    }

    fn apply_theme(&self, theme: &Theme) {
        self.themes.borrow_mut().push(theme.clone());
    }

    fn random_bytes(&self, buffer: &mut [u8]) {
        let next = self.randomness.get().wrapping_add(1);
        self.randomness.set(next);
        buffer.fill(next);
    }

    fn now_millis(&self) -> u64 {
        1_790_200_000_000
    }
}

fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    let waker = futures::task::noop_waker();
    Pin::new(future).poll(&mut TaskContext::from_waker(&waker))
}

fn done<F: Future + Unpin>(future: &mut F) -> F::Output {
    match poll(future) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("still pending"),
    }
}

fn init() -> Init {
    let mut tokens = BTreeMap::new();
    tokens.insert("--hlin-surface".to_string(), "#0f1115".to_string());
    Init {
        platform: "checklist".into(),
        panel: "items".into(),
        instance: "7f3c".into(),
        page: false,
        context: Context {
            time_range: Some(TimeRange {
                from_millis: 1,
                to_millis: 2,
            }),
            params: BTreeMap::from([("list".to_string(), vec!["team".to_string()])]),
            generation: 4,
        },
        theme: Theme {
            scheme: Scheme::Dark,
            tokens,
        },
        viewer: Viewer {
            name: Some("Alice".into()),
        },
        read_only: false,
        limits: Limits::default(),
        restored: None,
    }
}

fn from_shell(id: &str, message: ShellMessage) -> Envelope<ShellMessage> {
    Envelope::new(id, message)
}

fn answer(re: &str, response: Response) -> Envelope<ShellMessage> {
    Envelope::reply("s-99", re, ShellMessage::Response(response))
}

fn platform_answer(status: u16, body: &[u8]) -> Response {
    Response {
        status,
        headers: BTreeMap::new(),
        body: Some(body.to_vec()),
        refusal: None,
        streaming: false,
    }
}

/// A module that has had its `init`, and a host that has forgotten it.
fn started() -> (Module, Recorder) {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    module.receive(from_shell("s-1", ShellMessage::Init(init())));
    host.take();
    (module, host)
}

fn fetch_of(envelope: &Envelope<ModuleMessage>) -> &hlin_bridge::Fetch {
    match &envelope.message {
        ModuleMessage::Fetch(fetch) => fetch,
        other => panic!("expected a fetch, got {other:?}"),
    }
}

// --- The handshake ----------------------------------------------------------

#[test]
fn a_module_waits_for_init_and_then_knows_who_it_is() {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    let mut waiting = Box::pin(module.initialised());
    assert!(poll(&mut waiting).is_pending());
    assert_eq!(module.phase(), Phase::AwaitingInit);
    assert_eq!(module.identity(), None);

    module.receive(from_shell("s-1", ShellMessage::Init(init())));

    done(&mut waiting);
    assert_eq!(module.phase(), Phase::Initialised);
    let identity = module.identity().expect("known after init");
    assert_eq!(
        (identity.platform.as_str(), identity.panel.as_str()),
        ("checklist", "items")
    );
    assert!(host.take().is_empty(), "init is not answered; ready is");
    done(&mut Box::pin(module.initialised()));
}

#[test]
fn init_sets_every_signal_and_applies_the_theme() {
    let (module, host) = started();
    assert_eq!(module.context().get_untracked().generation, 4);
    assert_eq!(module.theme().get_untracked().scheme, Scheme::Dark);
    assert_eq!(
        module.viewer().get_untracked().name.as_deref(),
        Some("Alice")
    );
    assert!(!module.read_only().get_untracked());
    assert!(module.visible().get_untracked());
    assert_eq!(module.limits().get_untracked(), Limits::default());
    assert_eq!(host.themes.borrow().len(), 1);
    assert_eq!(host.themes.borrow()[0].tokens["--hlin-surface"], "#0f1115");
}

#[test]
fn ready_is_sent_once_naming_the_kit() {
    let (module, host) = started();
    module.ready(Some("aurora@0.2.1"));
    module.ready(Some("aurora@0.2.1"));
    let sent = host.only();
    assert_eq!(
        sent.message,
        ModuleMessage::Ready(hlin_bridge::Ready {
            kit: Some("aurora@0.2.1".into())
        })
    );
    assert_eq!(sent.bridge, hlin_bridge::VERSION);
    assert_eq!(module.phase(), Phase::Ready);
}

#[test]
fn ready_before_init_waits_for_it() {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    module.ready(None);
    assert!(host.take().is_empty());
    module.receive(from_shell("s-1", ShellMessage::Init(init())));
    assert_eq!(
        host.only().message,
        ModuleMessage::Ready(Default::default())
    );
    assert_eq!(module.phase(), Phase::Ready);
}

#[test]
fn a_heartbeat_is_echoed_as_a_reply_even_before_init() {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    module.receive(from_shell(
        "s-7",
        ShellMessage::Heartbeat(Heartbeat { n: 12 }),
    ));
    let echo = host.only();
    assert_eq!(echo.re.as_deref(), Some("s-7"));
    assert_eq!(echo.message, ModuleMessage::Heartbeat(Heartbeat { n: 12 }));
}

#[test]
fn nothing_but_init_and_the_heartbeat_counts_before_init() {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    module.receive(from_shell(
        "s-1",
        ShellMessage::Visibility(Visibility { visible: false }),
    ));
    assert!(module.visible().get_untracked());
    assert_eq!(module.phase(), Phase::AwaitingInit);
}

#[test]
fn a_second_init_is_ignored() {
    let (module, _) = started();
    let mut again = init();
    again.platform = "elsewhere".into();
    module.receive(from_shell("s-5", ShellMessage::Init(again)));
    assert_eq!(module.identity().unwrap().platform, "checklist");
}

#[test]
fn a_message_from_another_major_is_ignored() {
    let (module, host) = started();
    let mut envelope = from_shell("s-5", ShellMessage::Heartbeat(Heartbeat { n: 1 }));
    envelope.bridge = [2, 0];
    module.receive(envelope);
    assert!(host.take().is_empty());
}

#[test]
fn restored_bytes_from_init_are_offered_back() {
    let host = Recorder::default();
    let module = Module::new(host.clone());
    let mut with_state = init();
    with_state.restored = Some(b"draft".to_vec());
    module.receive(from_shell("s-1", ShellMessage::Init(with_state)));
    assert_eq!(module.restored(), Some(b"draft".to_vec()));
}

// --- What the shell says, as signals -----------------------------------------

#[test]
fn context_theme_and_visibility_update_their_signals() {
    let (module, host) = started();
    module.receive(from_shell(
        "s-2",
        ShellMessage::Context(Context {
            generation: 5,
            ..Context::default()
        }),
    ));
    module.receive(from_shell(
        "s-3",
        ShellMessage::Theme(Theme {
            scheme: Scheme::Light,
            tokens: BTreeMap::new(),
        }),
    ));
    module.receive(from_shell(
        "s-4",
        ShellMessage::Visibility(Visibility { visible: false }),
    ));
    assert_eq!(module.context().get_untracked().generation, 5);
    assert_eq!(module.theme().get_untracked().scheme, Scheme::Light);
    assert_eq!(host.themes.borrow().last().unwrap().scheme, Scheme::Light);
    assert!(!module.visible().get_untracked());
}

#[test]
fn every_relayed_change_is_numbered_so_repeats_still_wake_watchers() {
    let (module, _) = started();
    let changed = ShellChanged {
        panel: "items".into(),
        selections: BTreeMap::new(),
        from: ChangeSource::Module,
    };
    module.receive(from_shell("s-2", ShellMessage::Changed(changed.clone())));
    module.receive(from_shell("s-3", ShellMessage::Changed(changed)));
    let change = module.changes().get_untracked().expect("a change");
    assert_eq!(change.seq, 2);
    assert_eq!(change.from, ChangeSource::Module);
}

// --- Requests -----------------------------------------------------------------

#[test]
fn a_read_goes_out_without_a_key_and_its_answer_comes_back_to_it() {
    let (module, host) = started();
    let mut reply = Box::pin(
        module.fetch(
            Request::get("/api/lists/team/items")
                .query("done=false")
                .header("Accept", "application/json")
                .header("Authorization", "Bearer stolen"),
        ),
    );
    assert!(poll(&mut reply).is_pending());

    let sent = host.only();
    let fetch = fetch_of(&sent);
    assert_eq!(fetch.method, Method::Get);
    assert_eq!(fetch.path, "/api/lists/team/items");
    assert_eq!(fetch.query, "done=false");
    assert_eq!(fetch.idempotency_key, None);
    assert!(!fetch.stream);
    assert_eq!(
        fetch.headers.keys().collect::<Vec<_>>(),
        ["accept"],
        "only allowlisted headers cross"
    );

    module.receive(answer(&sent.id, platform_answer(200, b"[]")));
    let Reply::Answered(answer) = done(&mut reply).expect("answered") else {
        panic!("the platform answered")
    };
    assert!(answer.is_success());
    assert_eq!(answer.text(), "[]");
}

#[test]
fn the_platforms_refusal_is_an_answer_and_the_shells_is_a_refusal() {
    let (module, host) = started();
    let mut platform = Box::pin(module.fetch(Request::get("/api/a")));
    let mut shell = Box::pin(module.fetch(Request::get("/api/b")));
    let _ = poll(&mut platform);
    let _ = poll(&mut shell);
    let sent = host.take();

    // Answered out of order, to the right callers.
    module.receive(answer(
        &sent[1].id,
        Response {
            status: 404,
            headers: BTreeMap::new(),
            body: Some(b"outside the platform's prefixes".to_vec()),
            refusal: Some(Refusal::OutsidePrefix),
            streaming: false,
        },
    ));
    module.receive(answer(&sent[0].id, platform_answer(403, b"not yours")));

    let platform = done(&mut platform).unwrap();
    assert_eq!(platform.answered().map(|answer| answer.status), Some(403));
    let shell = done(&mut shell).unwrap();
    let refused = shell.refused().expect("the shell refused");
    assert_eq!(refused.refusal, Refusal::OutsidePrefix);
    assert_eq!(refused.reason, "outside the platform's prefixes");
}

#[test]
fn an_answer_to_nothing_is_ignored() {
    let (module, host) = started();
    module.receive(answer("m-404", platform_answer(200, b"")));
    assert!(host.take().is_empty());
}

#[test]
fn a_write_mints_a_key_and_a_retry_reuses_it() {
    let (module, host) = started();
    let attempt = module.attempt(
        Request::post("/api/lists/team/items/i1/toggle")
            .json(&true)
            .unwrap(),
    );
    let key = attempt
        .idempotency_key()
        .expect("writes carry a key")
        .to_string();
    assert_eq!(key.len(), 26);

    let _ = poll(&mut Box::pin(attempt.send()));
    let _ = poll(&mut Box::pin(attempt.retry()));
    let sent = host.take();
    assert_eq!(sent.len(), 2);
    assert_ne!(sent[0].id, sent[1].id, "each send is its own message");
    for envelope in &sent {
        let fetch = fetch_of(envelope);
        assert_eq!(fetch.idempotency_key.as_deref(), Some(key.as_str()));
        assert_eq!(fetch.body.as_deref(), Some(&b"true"[..]));
        assert_eq!(fetch.headers["content-type"], "application/json");
    }

    let another = module.attempt(Request::delete("/api/lists/team/items/i1"));
    assert_ne!(
        another.idempotency_key(),
        Some(key.as_str()),
        "a new write is a new attempt"
    );
}

#[test]
fn a_read_has_no_key_to_reuse() {
    let (module, _) = started();
    assert_eq!(
        module.attempt(Request::get("/api/x")).idempotency_key(),
        None
    );
    assert_eq!(
        module.attempt(Request::head("/api/x")).idempotency_key(),
        None
    );
}

// --- Streams ------------------------------------------------------------------

fn streaming_answer() -> Response {
    Response {
        status: 200,
        headers: BTreeMap::from([("content-type".to_string(), "text/plain".to_string())]),
        body: None,
        refusal: None,
        streaming: true,
    }
}

fn chunk(re: &str, seq: u64, body: &[u8]) -> Envelope<ShellMessage> {
    from_shell(
        "s-50",
        ShellMessage::Chunk(Chunk {
            re: re.into(),
            seq,
            body: Some(body.to_vec()),
        }),
    )
}

fn end(re: &str, error: Option<EndError>) -> Envelope<ShellMessage> {
    from_shell(
        "s-60",
        ShellMessage::End(End {
            re: re.into(),
            error,
        }),
    )
}

/// Opens a stream and returns its reader and the fetch's id, with the host's
/// record cleared.
fn open_stream(module: &Module, host: &Recorder) -> (BodyReader, String) {
    let mut opening = Box::pin(module.fetch_stream(Request::get("/api/logs")));
    assert!(poll(&mut opening).is_pending());
    let sent = host.only();
    assert!(fetch_of(&sent).stream);
    module.receive(answer(&sent.id, streaming_answer()));
    let StreamReply::Streaming { status, body, .. } = done(&mut opening).unwrap() else {
        panic!("a stream")
    };
    assert_eq!(status, 200);
    (body, sent.id)
}

fn pulls(sent: &[Envelope<ModuleMessage>]) -> Vec<u64> {
    sent.iter()
        .filter_map(|envelope| match &envelope.message {
            ModuleMessage::Pull(pull) => Some(pull.bytes),
            _ => None,
        })
        .collect()
}

#[test]
fn a_stream_grants_a_window_of_credit_and_tops_it_up_as_it_is_read() {
    let (module, host) = started();
    let (mut body, id) = open_stream(&module, &host);
    let sent = host.take();
    assert_eq!(pulls(&sent), [CREDIT_WINDOW]);
    match &sent[0].message {
        ModuleMessage::Pull(pull) => assert_eq!(pull.re, id),
        _ => unreachable!(),
    }

    module.receive(chunk(&id, 0, b"hello "));
    module.receive(chunk(&id, 1, b"world"));
    assert!(
        host.take().is_empty(),
        "credit is granted as bytes are read, not as they arrive"
    );

    assert_eq!(
        done(&mut Box::pin(body.next())),
        Ok(Some(b"hello ".to_vec()))
    );
    assert_eq!(pulls(&host.take()), [6]);
    assert_eq!(
        done(&mut Box::pin(body.next())),
        Ok(Some(b"world".to_vec()))
    );
    assert_eq!(pulls(&host.take()), [5]);

    module.receive(end(&id, None));
    assert_eq!(done(&mut Box::pin(body.next())), Ok(None));
    drop(body);
    assert!(host.take().is_empty(), "a finished stream is not cancelled");
}

#[test]
fn a_stream_waits_for_its_next_chunk() {
    let (module, host) = started();
    let (mut body, id) = open_stream(&module, &host);
    let mut next = Box::pin(body.next());
    assert!(poll(&mut next).is_pending());
    module.receive(chunk(&id, 0, b"late"));
    assert_eq!(done(&mut next), Ok(Some(b"late".to_vec())));
}

#[test]
fn dropping_a_reader_cancels_its_stream() {
    let (module, host) = started();
    let (body, id) = open_stream(&module, &host);
    host.take();
    drop(body);
    let cancel = host.only();
    assert_eq!(
        cancel.message,
        ModuleMessage::Cancel(hlin_bridge::Cancel { re: id.clone() })
    );

    // What was already on its way goes nowhere.
    module.receive(chunk(&id, 0, b"after"));
    assert!(host.take().is_empty());
}

#[test]
fn a_reader_dropped_after_the_end_arrived_does_not_cancel() {
    let (module, host) = started();
    let (body, id) = open_stream(&module, &host);
    host.take();
    module.receive(end(&id, None));
    drop(body);
    assert!(host.take().is_empty());
}

#[test]
fn a_stream_the_shell_ends_early_says_why() {
    let (module, host) = started();
    let (mut body, id) = open_stream(&module, &host);
    module.receive(end(&id, Some(EndError::Idle)));
    assert_eq!(
        done(&mut Box::pin(body.next())),
        Err(StreamError::Ended(EndError::Idle))
    );
    assert_eq!(done(&mut Box::pin(body.next())), Ok(None));
}

#[test]
fn a_refused_stream_is_a_refusal_not_a_reader() {
    let (module, host) = started();
    let mut opening = Box::pin(module.fetch_stream(Request::get("/api/logs")));
    let _ = poll(&mut opening);
    let sent = host.only();
    module.receive(answer(
        &sent.id,
        Response {
            status: 429,
            headers: BTreeMap::new(),
            body: Some(b"too many streams".to_vec()),
            refusal: Some(Refusal::TooMany),
            streaming: false,
        },
    ));
    assert!(matches!(
        done(&mut opening),
        Ok(StreamReply::Refused(Refused {
            refusal: Refusal::TooMany,
            ..
        }))
    ));
    assert!(
        host.take().is_empty(),
        "no credit for a stream that never opened"
    );
}

#[test]
fn a_write_cannot_be_streamed() {
    let (module, host) = started();
    let result = done(&mut Box::pin(module.fetch_stream(Request::post("/api/x"))));
    assert!(matches!(result, Err(BridgeError::StreamOnWrite)));
    assert!(host.take().is_empty());
}

// --- Suspend ------------------------------------------------------------------

#[test]
fn suspend_answers_with_what_the_hook_keeps() {
    let (module, host) = started();
    module.on_suspend(|| Some(b"half-typed".to_vec()));
    module.receive(from_shell(
        "s-8",
        ShellMessage::Suspend(Suspend { deadline_ms: 500 }),
    ));
    let state = host.only();
    assert_eq!(state.re.as_deref(), Some("s-8"));
    assert_eq!(
        state.message,
        ModuleMessage::State(hlin_bridge::State {
            blob: Some(b"half-typed".to_vec())
        })
    );
}

#[test]
fn suspend_without_a_hook_or_anything_to_keep_is_not_answered() {
    let (module, host) = started();
    module.receive(from_shell(
        "s-8",
        ShellMessage::Suspend(Suspend { deadline_ms: 500 }),
    ));
    assert!(host.take().is_empty());
    module.on_suspend(|| None);
    module.receive(from_shell(
        "s-9",
        ShellMessage::Suspend(Suspend { deadline_ms: 500 }),
    ));
    assert!(host.take().is_empty());
}

// --- What a module tells the shell ----------------------------------------------

#[test]
fn a_module_can_steer_the_surface_and_speak_in_its_panel() {
    let (module, host) = started();
    module.set_param("list", vec!["team".into()]);
    module.set_range(1, 2);
    module.navigate(Target {
        platform: "checklist".into(),
        page: Some("lists".into()),
        panel: None,
    });
    module.changed("items", BTreeMap::new());
    module.notice(NoticeLevel::Warning, &"x".repeat(200));

    let types: Vec<_> = host
        .take()
        .into_iter()
        .map(|envelope| {
            if let ModuleMessage::Notice(notice) = &envelope.message {
                assert_eq!(notice.text.chars().count(), 140);
            }
            hlin_bridge::Message::type_name(&envelope.message)
        })
        .collect();
    assert_eq!(
        types,
        ["set-param", "set-range", "navigate", "changed", "notice"]
    );
}

#[test]
fn every_message_a_module_sends_has_its_own_id() {
    let (module, host) = started();
    module.ready(None);
    module.set_range(1, 2);
    module.receive(from_shell(
        "s-3",
        ShellMessage::Heartbeat(Heartbeat { n: 1 }),
    ));
    let ids: Vec<_> = host
        .take()
        .into_iter()
        .map(|envelope| envelope.id)
        .collect();
    assert_eq!(ids, ["m-1", "m-2", "m-3"]);
}
