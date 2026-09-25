//! Every message against the JSON specification HLIN-S-0007 shows for it.
//!
//! Each case is read, then written back, and must come out as it went in,
//! minus the `"<ArrayBuffer>"` placeholders: bytes ride beside the JSON, never
//! in it.

use serde_json::{Value, json};

use super::*;

fn without_placeholders(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(_, field)| field.as_str() != Some("<ArrayBuffer>"))
                .map(|(key, field)| (key.clone(), without_placeholders(field)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn round_trips<M: Message + std::fmt::Debug>(wire: Value) -> Envelope<M> {
    let envelope = match Envelope::<M>::from_json(&wire) {
        Decoded::Message(envelope) => envelope,
        other => panic!("expected a message from {wire}, got {other:?}"),
    };
    assert_eq!(envelope.to_json(), without_placeholders(&wire));
    envelope
}

fn shell(type_name: &str, data: Value) -> Envelope<ShellMessage> {
    round_trips(json!({ "bridge": [1, 0], "id": "s-2", "type": type_name, "data": data }))
}

fn module(type_name: &str, data: Value) -> Envelope<ModuleMessage> {
    round_trips(json!({ "bridge": [1, 0], "id": "m-2", "type": type_name, "data": data }))
}

// --- Shell to module -------------------------------------------------------

#[test]
fn init_reads_and_writes_as_the_specification_shows() {
    let envelope: Envelope<ShellMessage> = round_trips(json!({
        "bridge": [1, 0], "id": "s-1", "type": "init", "data": {
            "platform": "checklist", "panel": "items", "instance": "7f3c",
            "page": false,
            "context": {
                "time_range": { "from_millis": 1790200000000_i64, "to_millis": 1790286400000_i64 },
                "params": { "list": ["team"] },
                "generation": 4
            },
            "theme": { "scheme": "dark", "tokens": { "--hlin-surface": "#0f1115", "--hlin-accent": "#7aa2f7" } },
            "viewer": { "name": "Alice" },
            "read_only": false,
            "limits": { "request_bytes": 1048576, "response_bytes": 4194304,
                        "fetches_in_flight": 8, "streams": 2 },
            "restored": null
        }
    }));
    let ShellMessage::Init(init) = envelope.message else {
        panic!("not init")
    };
    assert_eq!(init.theme.scheme, Scheme::Dark);
    assert_eq!(init.context.params["list"], ["team"]);
    assert_eq!(init.limits, Limits::default());
    assert_eq!(init.restored, None);
}

#[test]
fn context_theme_and_visibility_read_and_write() {
    shell(
        "context",
        json!({ "time_range": { "from_millis": 1, "to_millis": 2 }, "params": {}, "generation": 5 }),
    );
    shell(
        "context",
        json!({ "time_range": null, "params": { "list": ["a", "b"] }, "generation": 0 }),
    );
    shell(
        "theme",
        json!({ "scheme": "light", "tokens": { "--hlin-text": "#111" } }),
    );
    shell("visibility", json!({ "visible": false }));
}

#[test]
fn changed_from_the_shell_names_where_it_came_from() {
    let envelope = shell(
        "changed",
        json!({ "panel": "items", "selections": { "list": ["team"] }, "from": "platform" }),
    );
    assert!(matches!(
        envelope.message,
        ShellMessage::Changed(ShellChanged {
            from: ChangeSource::Platform,
            ..
        })
    ));
    shell(
        "changed",
        json!({ "panel": "items", "selections": {}, "from": "module" }),
    );
}

#[test]
fn a_response_carries_its_refusal_as_null_when_the_platform_answered() {
    let envelope: Envelope<ShellMessage> = round_trips(json!({
        "bridge": [1, 0], "id": "s-41", "re": "m-17", "type": "response", "data": {
            "status": 403,
            "headers": { "content-type": "application/json" },
            "body": "<ArrayBuffer>",
            "refusal": null
        }
    }));
    assert_eq!(envelope.re.as_deref(), Some("m-17"));
    let ShellMessage::Response(response) = envelope.message else {
        panic!("not response")
    };
    assert_eq!(response.refusal, None);
    assert!(!response.streaming);
}

#[test]
fn a_refused_response_names_the_refusal() {
    for (code, refusal) in [
        ("not_from_shell", Refusal::NotFromShell),
        ("not_signed_in", Refusal::NotSignedIn),
        ("outside_prefix", Refusal::OutsidePrefix),
        ("method", Refusal::Method),
        ("read_only", Refusal::ReadOnly),
        ("no_identity", Refusal::NoIdentity),
        ("no_idempotency_key", Refusal::NoIdempotencyKey),
        ("too_large", Refusal::TooLarge),
        ("unreachable", Refusal::Unreachable),
        ("timeout", Refusal::Timeout),
        ("too_many", Refusal::TooMany),
    ] {
        let envelope = shell(
            "response",
            json!({ "status": 429, "headers": {}, "body": "<ArrayBuffer>", "refusal": code }),
        );
        let ShellMessage::Response(response) = envelope.message else {
            panic!("not response")
        };
        assert_eq!(response.refusal, Some(refusal));
    }
}

#[test]
fn a_refusal_code_from_a_newer_minor_is_still_a_refusal() {
    let wire = json!({ "bridge": [1, 4], "id": "s-3", "re": "m-1", "type": "response",
                       "data": { "status": 403, "headers": {}, "refusal": "quota" } });
    let envelope = Envelope::<ShellMessage>::from_json(&wire)
        .message()
        .expect("a response");
    let ShellMessage::Response(response) = envelope.message else {
        panic!("not response")
    };
    assert_eq!(response.refusal, Some(Refusal::Unrecognised));
}

#[test]
fn a_streamed_response_says_so_and_carries_no_body() {
    shell(
        "response",
        json!({ "status": 200, "headers": { "content-type": "text/plain" }, "refusal": null, "streaming": true }),
    );
}

#[test]
fn heartbeat_chunk_end_and_suspend_read_and_write() {
    shell("heartbeat", json!({ "n": 12 }));
    shell(
        "chunk",
        json!({ "re": "m-17", "seq": 0, "body": "<ArrayBuffer>" }),
    );
    shell("end", json!({ "re": "m-17" }));
    for reason in ["idle", "rate", "unreachable", "cancelled", "unmounted"] {
        shell("end", json!({ "re": "m-17", "error": reason }));
    }
    shell("suspend", json!({ "deadline_ms": 500 }));
}

// --- Module to shell -------------------------------------------------------

#[test]
fn ready_names_the_kit_only_when_there_is_one() {
    module("ready", json!({ "kit": "aurora@0.2.1" }));
    module("ready", json!({}));
}

#[test]
fn fetch_reads_and_writes_as_the_specification_shows() {
    let envelope: Envelope<ModuleMessage> = round_trips(json!({
        "bridge": [1, 0], "id": "m-17", "type": "fetch", "data": {
            "method": "POST", "path": "/api/lists/team/items",
            "query": "", "headers": { "content-type": "application/json" },
            "body": "<ArrayBuffer>", "idempotency_key": "01J8Z"
        }
    }));
    let ModuleMessage::Fetch(fetch) = envelope.message else {
        panic!("not fetch")
    };
    assert!(fetch.method.is_write());
    assert!(!fetch.stream);
}

#[test]
fn a_streamed_read_asks_for_its_body_as_it_arrives() {
    module(
        "fetch",
        json!({ "method": "GET", "path": "/api/logs", "query": "tail=1", "headers": {}, "stream": true }),
    );
}

#[test]
fn a_method_the_bridge_does_not_carry_still_reads_so_the_page_can_refuse_it() {
    let wire = json!({ "bridge": [1, 0], "id": "m-3", "type": "fetch",
                       "data": { "method": "OPTIONS", "path": "/api/x" } });
    let envelope = Envelope::<ModuleMessage>::from_json(&wire)
        .message()
        .expect("a fetch");
    let ModuleMessage::Fetch(fetch) = envelope.message else {
        panic!("not fetch")
    };
    assert_eq!(fetch.method, Method::Other);
    assert!(!fetch.method.is_read() && !fetch.method.is_write());
}

#[test]
fn the_remaining_module_messages_read_and_write() {
    module("set-param", json!({ "id": "list", "values": ["team"] }));
    module("set-range", json!({ "from_millis": 1, "to_millis": 2 }));
    module(
        "navigate",
        json!({ "to": { "platform": "checklist", "page": "lists" } }),
    );
    module(
        "navigate",
        json!({ "to": { "platform": "checklist", "panel": "items" } }),
    );
    module(
        "changed",
        json!({ "panel": "items", "selections": { "list": ["team"] } }),
    );
    for level in ["info", "warning", "error"] {
        module("notice", json!({ "level": level, "text": "Sync paused" }));
    }
    module("heartbeat", json!({ "n": 12 }));
    module("pull", json!({ "re": "m-17", "bytes": 262144 }));
    module("cancel", json!({ "re": "m-17" }));
    module("state", json!({ "blob": "<ArrayBuffer>" }));
}

// --- The envelope ----------------------------------------------------------

#[test]
fn every_known_type_has_a_variant_that_writes_it() {
    for type_name in ShellMessage::TYPES {
        assert_eq!(
            ShellMessage::TYPES
                .iter()
                .filter(|t| *t == type_name)
                .count(),
            1
        );
    }
    for type_name in ModuleMessage::TYPES {
        assert_eq!(
            ModuleMessage::TYPES
                .iter()
                .filter(|t| *t == type_name)
                .count(),
            1
        );
    }
    let sent = [
        ModuleMessage::Ready(Ready::default()),
        ModuleMessage::SetRange(TimeRange {
            from_millis: 0,
            to_millis: 1,
        }),
        ModuleMessage::State(State::default()),
    ];
    for message in sent {
        assert!(ModuleMessage::TYPES.contains(&message.to_parts().0));
    }
}

#[test]
fn a_type_from_a_newer_minor_is_unknown_not_an_error() {
    let wire = json!({ "bridge": [1, 7], "id": "s-9", "type": "sparkle", "data": { "x": 1 } });
    assert_eq!(
        Envelope::<ShellMessage>::from_json(&wire),
        Decoded::Unknown {
            bridge: [1, 7],
            id: "s-9".into(),
            type_name: "sparkle".into()
        }
    );
}

#[test]
fn unknown_fields_are_ignored_at_every_level() {
    let wire = json!({ "bridge": [1, 2], "id": "s-5", "later": true, "type": "visibility",
                       "data": { "visible": true, "why": "scrolled" } });
    let envelope = Envelope::<ShellMessage>::from_json(&wire)
        .message()
        .expect("visibility");
    assert_eq!(envelope.bridge, [1, 2]);
    assert_eq!(
        envelope.message,
        ShellMessage::Visibility(Visibility { visible: true })
    );
}

#[test]
fn something_without_the_envelope_shape_is_malformed() {
    for wire in [
        json!("ready"),
        json!({ "id": "m-1", "type": "ready", "data": {} }),
        json!({ "bridge": [1], "id": "m-1", "type": "ready", "data": {} }),
        json!({ "bridge": [1, -1], "id": "m-1", "type": "ready", "data": {} }),
        json!({ "bridge": [1, 0], "type": "ready", "data": {} }),
        json!({ "bridge": [1, 0], "id": "m-1", "data": {} }),
        json!({ "bridge": [1, 0], "id": "m-1", "type": "ready" }),
        json!({ "bridge": [1, 0], "id": "m-1", "type": "ready", "data": [] }),
        json!({ "bridge": [1, 0], "id": "m-1", "re": 3, "type": "ready", "data": {} }),
    ] {
        assert!(
            matches!(
                Envelope::<ModuleMessage>::from_json(&wire),
                Decoded::Malformed(_)
            ),
            "{wire} should be malformed"
        );
    }
}

#[test]
fn a_known_type_with_fields_that_do_not_fit_is_malformed() {
    let wire =
        json!({ "bridge": [1, 0], "id": "m-1", "type": "heartbeat", "data": { "n": "twelve" } });
    assert!(matches!(
        Envelope::<ModuleMessage>::from_json(&wire),
        Decoded::Malformed(_)
    ));
}

#[test]
fn a_reply_names_what_it_answers() {
    let reply = Envelope::reply("m-4", "s-3", ModuleMessage::Heartbeat(Heartbeat { n: 3 }));
    assert_eq!(
        reply.to_json(),
        json!({ "bridge": [1, 0], "id": "m-4", "re": "s-3", "type": "heartbeat", "data": { "n": 3 } })
    );
}

#[test]
fn bytes_live_in_the_field_the_specification_names() {
    assert_eq!(ShellMessage::bytes_field("init"), Some("restored"));
    assert_eq!(ShellMessage::bytes_field("response"), Some("body"));
    assert_eq!(ShellMessage::bytes_field("chunk"), Some("body"));
    assert_eq!(ShellMessage::bytes_field("heartbeat"), None);
    assert_eq!(ModuleMessage::bytes_field("fetch"), Some("body"));
    assert_eq!(ModuleMessage::bytes_field("state"), Some("blob"));

    let mut state = ModuleMessage::State(State::default());
    state.set_bytes(vec![1, 2, 3]);
    assert_eq!(state.bytes(), Some(&[1, 2, 3][..]));
    assert_eq!(state.take_bytes(), Some(vec![1, 2, 3]));
    assert_eq!(state.bytes(), None);
}

// --- Constants -------------------------------------------------------------

#[test]
fn this_crate_speaks_a_major_the_shell_supports() {
    assert!(SUPPORTED_BRIDGE_MAJORS.contains(&MAJOR));
    assert_eq!(VERSION, [1, 0]);
}

#[test]
fn request_headers_are_checked_against_the_allowlist_ignoring_case() {
    assert!(is_allowed_request_header("Content-Type"));
    assert!(is_allowed_request_header("if-none-match"));
    assert!(!is_allowed_request_header("authorization"));
    assert!(!is_allowed_request_header("cookie"));
}

#[test]
fn ids_are_prefixed_by_side_and_never_repeat() {
    let mut module = IdMint::module();
    let mut shell = IdMint::shell();
    assert_eq!(module.mint(), "m-1");
    assert_eq!(module.mint(), "m-2");
    assert_eq!(shell.mint(), "s-1");
}
