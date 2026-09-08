//! Listening to the shell.
//!
//! Server-sent events in, frames out, plus the one thing the browser must
//! decide for itself: that the shell has gone away.

use wasm_bindgen::prelude::*;
use web_sys::{EventSource, MessageEvent};

use hlin_stream::{Frame, PanelFrame, SurfaceFrame};

/// Open a stream and call `on_frame` for everything that arrives.
///
/// `on_lost` fires when the connection drops, which is the browser's cue to
/// mark everything stale. The `EventSource` reconnects by itself, and the
/// shell resends the current state of every panel when it does, so nothing
/// here tries to repair anything.
pub fn open(
    url: &str,
    on_frame: impl Fn(Frame) + 'static,
    on_lost: impl Fn() + 'static,
    on_open: impl Fn() + 'static,
) -> Result<EventSource, JsValue> {
    let source = EventSource::new(url)?;
    let deliver = std::rc::Rc::new(on_frame);

    let panel_handler = Closure::<dyn Fn(MessageEvent)>::new({
        let deliver = deliver.clone();
        move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };
            // An unreadable frame is skipped rather than fatal: the protocol is
            // additive, and a browser older than its shell must keep working.
            if let Ok(frame) = serde_json::from_str::<PanelFrame>(&text) {
                deliver(Frame::Panel(Box::new(frame)));
            }
        }
    });
    source.add_event_listener_with_callback("panel", panel_handler.as_ref().unchecked_ref())?;
    panel_handler.forget();

    // The surface frame is how the shell says it received a parameter change,
    // which is the only thing that can tell a viewer their picker did
    // something before any panel has answered.
    let surface_handler = Closure::<dyn Fn(MessageEvent)>::new({
        let deliver = deliver.clone();
        move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };
            if let Ok(frame) = serde_json::from_str::<SurfaceFrame>(&text) {
                deliver(Frame::Surface(frame));
            }
        }
    });
    source.add_event_listener_with_callback("surface", surface_handler.as_ref().unchecked_ref())?;
    surface_handler.forget();

    let error_handler = Closure::<dyn Fn(JsValue)>::new(move |_| on_lost());
    source.set_onerror(Some(error_handler.as_ref().unchecked_ref()));
    error_handler.forget();

    let open_handler = Closure::<dyn Fn(JsValue)>::new(move |_| on_open());
    source.set_onopen(Some(open_handler.as_ref().unchecked_ref()));
    open_handler.forget();

    Ok(source)
}

/// Tell the shell a control moved.
pub async fn set_params(surface: &str, request: &hlin_stream::ParamsRequest) -> Result<(), String> {
    gloo_net::http::Request::post(&format!("/api/stream/{surface}/params"))
        .header("content-type", "application/json")
        .body(serde_json::to_string(request).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
