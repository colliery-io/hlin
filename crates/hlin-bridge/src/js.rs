//! Messages as the JavaScript objects `postMessage` carries.
//!
//! The JSON goes through `JSON.parse`/`JSON.stringify`, which is plain data
//! either way, and the one field that carries bytes is put in or taken out as
//! an `ArrayBuffer`. The sender passes the returned transfer list to
//! `postMessage`, so the bytes move rather than being copied.

use js_sys::{Array, ArrayBuffer, JSON, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

use crate::{Decoded, Envelope, Message};

/// A message as a JavaScript object, and the list of buffers to transfer
/// with it (empty when it carries no bytes).
pub fn to_js<M: Message>(envelope: &Envelope<M>) -> (JsValue, Array) {
    let text = envelope.to_json().to_string();
    let value = JSON::parse(&text).expect("serde_json writes JSON");
    let transfer = Array::new();
    if let Some(bytes) = envelope.message.bytes()
        && let Some(field) = M::bytes_field(envelope.message.type_name())
    {
        let buffer = Uint8Array::from(bytes).buffer();
        if let Ok(data) = Reflect::get(&value, &JsValue::from_str("data")) {
            let _ = Reflect::set(&data, &JsValue::from_str(field), &buffer);
            transfer.push(&buffer);
        }
    }
    (value, transfer)
}

/// Reads a message from what `postMessage` delivered. Never throws: anything
/// that is not an envelope is [`Decoded::Malformed`].
pub fn from_js<M: Message>(value: &JsValue) -> Decoded<M> {
    if !value.is_object() {
        return Decoded::Malformed("not an object".into());
    }
    let bytes = Reflect::get(value, &JsValue::from_str("type"))
        .ok()
        .and_then(|type_name| type_name.as_string())
        .and_then(|type_name| M::bytes_field(&type_name))
        .and_then(|field| {
            let data = Reflect::get(value, &JsValue::from_str("data")).ok()?;
            if !data.is_object() {
                return None;
            }
            let found = Reflect::get(&data, &JsValue::from_str(field)).ok()?;
            let buffer = found.dyn_into::<ArrayBuffer>().ok()?;
            Some(Uint8Array::new(&buffer).to_vec())
        });
    // An ArrayBuffer stringifies as `{}`, which the serde form ignores, so
    // the bytes field needs no removing first.
    let Some(text) = JSON::stringify(value)
        .ok()
        .and_then(|text| text.as_string())
    else {
        return Decoded::Malformed("not JSON".into());
    };
    let json = match serde_json::from_str(&text) {
        Ok(json) => json,
        Err(error) => return Decoded::Malformed(error.to_string()),
    };
    match Envelope::<M>::from_json(&json) {
        Decoded::Message(mut envelope) => {
            if let Some(bytes) = bytes {
                envelope.message.set_bytes(bytes);
            }
            Decoded::Message(envelope)
        }
        other => other,
    }
}
