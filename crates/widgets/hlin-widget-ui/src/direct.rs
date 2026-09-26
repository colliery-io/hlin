//! The client a widget's own UI uses, at the root of its own origin.
//!
//! Same-origin `fetch` to the widget's `/api/`, and the widget's own event
//! stream (`/api/events`, by `EventSource`) for news that something changed:
//! the same stream Hlin subscribes to on the platform's behalf, under
//! `/hlin/`. So one person's bump in the widget's own page reaches the Hlin
//! module on somebody's surface, and the other way round, with nothing
//! between them but the platform.
//!
//! Who is asking is the platform's business, not this client's: it sends no
//! credential, and the platform decides who a request is from its own sign-in.
//! In the demo that is `--local-user`, one fixed person for the whole of
//! `/api/`, which is why the demo's widgets are never reachable from anyone
//! else's machine.

use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;

use crate::client::{
    Answer, Attempt, Borrowed, Chunks, Client, Ended, Pending, Reply, Request, Streamed, TimeRange,
    Trouble,
};

/// Mount `app`, a widget's components, as the widget's own UI.
pub fn mount<F, V>(app: F)
where
    F: FnOnce() -> V + 'static,
    V: IntoView + 'static,
{
    console_error_panic_hook::set_once();
    crate::mount(Direct::new(), app);
}

/// The widget's own UI's client (see the module docs).
pub struct Direct {
    changes: ArcRwSignal<u64>,
    visible: ArcRwSignal<bool>,
    // Held so the stream stays open for the life of the page.
    _events: Option<web_sys::EventSource>,
}

impl Default for Direct {
    fn default() -> Self {
        Self::new()
    }
}

impl Direct {
    /// A client for the page it is on, listening to the widget's event
    /// stream from now on.
    pub fn new() -> Self {
        let changes = ArcRwSignal::new(0_u64);
        let events = web_sys::EventSource::new("/api/events").ok();
        if let Some(events) = &events {
            let bump = changes.clone();
            let changed = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                bump.update(|count| *count += 1);
            });
            let _ = events
                .add_event_listener_with_callback("changed", changed.as_ref().unchecked_ref());
            changed.forget();

            // `EventSource` reconnects by itself. An open after the first is
            // the stream coming back, and whatever changed while it was gone
            // was never announced, so it counts as a change.
            let opened = Rc::new(Cell::new(false));
            let bump = changes.clone();
            let open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                if opened.replace(true) {
                    bump.update(|count| *count += 1);
                }
            });
            events.set_onopen(Some(open.as_ref().unchecked_ref()));
            open.forget();
        }

        let visible = ArcRwSignal::new(page_visible());
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let seen = visible.clone();
            let changed = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                seen.set(page_visible());
            });
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                changed.as_ref().unchecked_ref(),
            );
            changed.forget();
        }

        Self {
            changes,
            visible,
            _events: events,
        }
    }
}

impl Client for Direct {
    fn fetch(&self, request: Request) -> Pending<Reply> {
        Box::pin(async move {
            let response = call(&request, None).await?;
            Ok(gather(response).await)
        })
    }

    fn attempt(&self, request: Request) -> Box<dyn Attempt> {
        Box::new(DirectAttempt {
            request,
            key: idempotency_key(),
        })
    }

    fn stream(&self, request: Request) -> Pending<Streamed> {
        Box::pin(async move {
            let response = match call(&request, None).await {
                Ok(response) => response,
                Err(trouble) => return Streamed::Trouble(trouble),
            };
            if !response.ok() {
                return Streamed::Answered(gather(response).await);
            }
            let reader = response
                .body()
                .and_then(|body| body.get_reader().dyn_into().ok());
            match reader {
                Some(reader) => Streamed::Streaming(Box::new(Body { reader })),
                None => Streamed::Answered(Answer {
                    status: response.status(),
                    body: Vec::new(),
                }),
            }
        })
    }

    fn changes(&self) -> Signal<u64> {
        self.changes.read_only().into()
    }

    /// Nobody to tell: the widget's event stream tells every other browser,
    /// this one included.
    fn announce(&self) {}

    /// The platform decides; this page offers everything and shows its
    /// refusals.
    fn read_only(&self) -> Signal<bool> {
        Signal::stored(false)
    }

    fn visible(&self) -> Signal<bool> {
        self.visible.read_only().into()
    }

    /// The widget's own page has no time picker.
    fn time_range(&self) -> Signal<Option<TimeRange>> {
        Signal::stored(None)
    }

    /// A page is never suspended, so there is never anything to bring back.
    fn restored(&self) -> Option<Vec<u8>> {
        None
    }

    fn on_suspend(&self, _keep: Box<dyn Fn() -> Option<Vec<u8>>>) {}
}

/// A write, holding the key it is sent with every time.
struct DirectAttempt {
    request: Request,
    key: String,
}

impl Attempt for DirectAttempt {
    fn send(&self) -> Pending<Reply> {
        let request = self.request.clone();
        let key = self.key.clone();
        Box::pin(async move {
            let response = call(&request, Some(&key)).await?;
            Ok(gather(response).await)
        })
    }
}

/// A streamed body, read as it arrives.
struct Body {
    reader: web_sys::ReadableStreamDefaultReader,
}

impl Chunks for Body {
    fn next(&mut self) -> Borrowed<'_, Result<Option<Vec<u8>>, Ended>> {
        Box::pin(async move {
            let read = JsFuture::from(self.reader.read())
                .await
                .map_err(|_| Ended::Unreachable)?;
            let done = js_sys::Reflect::get(&read, &"done".into())
                .ok()
                .and_then(|done| done.as_bool())
                .unwrap_or(true);
            if done {
                return Ok(None);
            }
            let value = js_sys::Reflect::get(&read, &"value".into()).map_err(|_| Ended::Gone)?;
            Ok(Some(js_sys::Uint8Array::new(&value).to_vec()))
        })
    }
}

impl Drop for Body {
    /// Stop the request, rather than leave the platform writing to nobody.
    fn drop(&mut self) {
        let _ = self.reader.cancel();
    }
}

/// Send `request` to this origin, with `key` as its idempotency key.
async fn call(request: &Request, key: Option<&str>) -> Result<web_sys::Response, Trouble> {
    let unreachable = || Trouble {
        words: "The widget could not be reached. Try again in a moment.".to_string(),
        retry: true,
    };
    let window = web_sys::window().ok_or_else(unreachable)?;
    let init = web_sys::RequestInit::new();
    init.set_method(request.method.as_str());
    let headers = web_sys::Headers::new().map_err(|_| unreachable())?;
    if let Some(content_type) = &request.content_type {
        let _ = headers.set("content-type", content_type);
    }
    if let Some(key) = key {
        let _ = headers.set("idempotency-key", key);
    }
    init.set_headers(&headers);
    if let Some(body) = &request.body {
        init.set_body(&js_sys::Uint8Array::from(body.as_slice()));
    }
    let response = JsFuture::from(window.fetch_with_str_and_init(&request.target(), &init))
        .await
        .map_err(|_| unreachable())?;
    response.dyn_into().map_err(|_| unreachable())
}

/// A whole answer, whatever its status.
async fn gather(response: web_sys::Response) -> Answer {
    let status = response.status();
    let body = match response.array_buffer() {
        Ok(buffer) => JsFuture::from(buffer)
            .await
            .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    Answer { status, body }
}

/// A key nobody else will use: 128 random bits, in hex.
fn idempotency_key() -> String {
    let mut bytes = [0_u8; 16];
    let random = web_sys::window()
        .and_then(|window| window.crypto().ok())
        .map(|crypto| crypto.get_random_values_with_u8_array(&mut bytes).is_ok())
        .unwrap_or(false);
    if !random {
        for byte in &mut bytes {
            *byte = (js_sys::Math::random() * 256.0) as u8;
        }
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn page_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_none_or(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}
