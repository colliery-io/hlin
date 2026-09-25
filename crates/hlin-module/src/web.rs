//! The browser: the frame's parent, its document, and its crypto.

use std::cell::RefCell;
use std::collections::BTreeSet;

use hlin_bridge::js::{from_js, to_js};
use hlin_bridge::{Decoded, Envelope, ModuleMessage, Scheme, ShellMessage, Theme};
use js_sys::Object;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlElement, MessageEvent, Window};

use crate::{Host, Module};

/// The host a module in a browser frame runs on.
pub struct WebHost {
    parent: Option<Window>,
    applied: RefCell<BTreeSet<String>>,
}

impl WebHost {
    /// A host posting to `parent`: the shell's page. With no parent (a module
    /// opened outside a frame) messages go nowhere and `init` never comes.
    pub fn new(parent: Option<Window>) -> Self {
        Self {
            parent,
            applied: RefCell::new(BTreeSet::new()),
        }
    }
}

impl Host for WebHost {
    /// Target origin `*`, because the frame's origin is opaque and there is no
    /// origin to name for the parent either that the frame could verify. That
    /// is safe because nothing a module sends is secret from its own page
    /// (REQ-2.2), and what receives it decides who sent it from which frame
    /// it came from, not from anything in the message.
    fn post(&self, envelope: Envelope<ModuleMessage>) {
        let Some(parent) = &self.parent else {
            return;
        };
        let (value, transfer) = to_js(&envelope);
        let _ = parent.post_message_with_transfer(&value, "*", &transfer);
    }

    /// Tokens become custom properties on the document root, so a module's
    /// CSS reads `var(--hlin-surface)` and follows the shell without asking. Tokens
    /// the shell stops sending are removed, and only `--` names are applied:
    /// the shell's theme is tokens, not arbitrary style.
    fn apply_theme(&self, theme: &Theme) {
        let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let style = root.style();
        let mut applied = self.applied.borrow_mut();
        for gone in applied
            .iter()
            .filter(|name| !theme.tokens.contains_key(*name))
        {
            let _ = style.remove_property(gone);
        }
        applied.clear();
        for (name, value) in theme
            .tokens
            .iter()
            .filter(|(name, _)| name.starts_with("--"))
        {
            let _ = style.set_property(name, value);
            applied.insert(name.clone());
        }
        let scheme = match theme.scheme {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        };
        let _ = style.set_property("color-scheme", scheme);
        let _ = root.set_attribute("data-hl-scheme", scheme);
    }

    fn random_bytes(&self, buffer: &mut [u8]) {
        let filled = web_sys::window()
            .and_then(|window| window.crypto().ok())
            .is_some_and(|crypto| crypto.get_random_values_with_u8_array(buffer).is_ok());
        if !filled {
            for byte in buffer.iter_mut() {
                *byte = (js_sys::Math::random() * 256.0) as u8;
            }
        }
    }

    fn now_millis(&self) -> u64 {
        js_sys::Date::now() as u64
    }
}

/// Connects to the shell's page and waits for `init`.
///
/// Listens for messages from the frame's parent and nothing else: a message
/// whose `source` is not `window.parent` is dropped before it is read, so
/// nothing but the shell's page can drive the module. Call once, as the
/// module starts, before anything is drawn; then call [`Module::ready`] once
/// it can draw.
///
/// In a debug build it also starts watching the frame's main thread, and warns
/// in the console when the module holds it for 200 ms or more (see the crate
/// docs, *Never block the main thread*). A release build does not.
pub async fn connect() -> Module {
    let window = web_sys::window().expect("a module runs in a browser window");
    #[cfg(debug_assertions)]
    crate::watch::start();
    let parent = window.parent().ok().flatten();
    let module = Module::new(WebHost::new(parent.clone()));

    let receiver = module.clone();
    let listener = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let (Some(parent), Some(source)) = (&parent, event.source()) else {
            return;
        };
        if !Object::is(source.as_ref(), parent.as_ref()) {
            return;
        }
        if let Decoded::Message(envelope) = from_js::<ShellMessage>(&event.data()) {
            receiver.receive(envelope);
        }
    });
    let _ = window.add_event_listener_with_callback("message", listener.as_ref().unchecked_ref());
    // The listener lives as long as the frame does.
    listener.forget();

    module.initialised().await;
    module
}
