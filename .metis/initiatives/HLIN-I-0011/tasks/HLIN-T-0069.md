---
id: an-sdk-so-a-leptos-module-does-not
level: task
title: "An SDK so a Leptos module does not know it is in a frame"
short_code: "HLIN-T-0069"
created_at: 2026-09-25T00:01:07.895454+00:00
updated_at: 2026-09-25T00:53:58.774384+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# An SDK so a Leptos module does not know it is in a frame

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 9 of [[HLIN-I-0011]]. NFR-1.2 of [[HLIN-S-0007]]: a module built with
the SDK needs no bridge code of its own.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] A new published-shape crate (name to settle, e.g. `hlin-module`) for
      Leptos 0.8 modules compiled to `wasm32`
- [x] Handles the handshake: waits for `init`, exposes `ready()`, answers
      `heartbeat` automatically
- [x] Context, theme, visibility and viewer as signals; theme tokens applied
      to the frame's document
- [x] `fetch` as an async call returning status, headers and body, with the
      shell's refusal distinguishable from the platform's answer; writes mint
      an idempotency key per attempt and reuse it on an explicit retry
- [x] Streams as an async reader that pulls credit as it is consumed, and
      cancels when dropped ([[HLIN-T-0068]])
- [x] `set_param`, `set_range`, `navigate`, `changed`, `notice` as functions
- [x] A `suspend` hook returning bytes, and the restored bytes at startup
- [x] Message types shared with the shell page, not duplicated
- [ ] Unit tests for encoding and the handshake; used by [[HLIN-T-0071]]'s
      sample module
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Built alongside [[HLIN-T-0066]]: each is the other's test.
- Put the wire types in one crate both sides depend on (an extension of
  `hlin-stream`, or a new small crate), so the protocol cannot drift.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 (implemented)

**What.** Two new crates, both `cargo check --target wasm32-unknown-unknown`
clean, 23 + 30 native unit tests and two compiled doc examples.

- `hlin-bridge`: the envelope (`Envelope<M>`, `Decoded<M>`), every message in
  both directions (`ShellMessage`, `ModuleMessage` and their data structs),
  `VERSION = [1, 0]`, the header allowlists, the timing constants, and
  `IdMint`. Every message is round-tripped against the specification's JSON.
  A `js` feature converts to and from the object `postMessage` carries,
  putting bytes in as a transferable `ArrayBuffer` (`js::to_js` returns the
  transfer list) and taking them out of one (`js::from_js`).
- `hlin-module`: the Leptos 0.8 SDK. `connect().await` listens only to
  `window.parent` and resolves on `init`; `Module::ready(kit)`; heartbeat
  echoed automatically (even before `init`); `context`, `theme`, `visible`,
  `viewer`, `read_only`, `limits`, `changes` as `Signal`s; theme tokens set as
  custom properties on `<html>` (stale ones removed, only `--` names, plus
  `color-scheme` and `data-hl-scheme`); `fetch`/`attempt` returning
  `Reply::Answered | Reply::Refused`; `fetch_stream` returning a `BodyReader`
  with credit and cancel-on-drop; `set_param`, `set_range`, `navigate`,
  `changed`, `notice`; `on_suspend` and `restored`. The browser sits behind a
  `Host` trait so the protocol is tested natively.

**Decisions.**

- *A new crate rather than a module of `hlin-stream`.* `hlin-stream` pulls in
  `hlin-view` and the manifest's contract machinery (sha2, semver, JCS) to say
  what the shell tells its own browser. A platform team's module should cost
  `serde` and nothing else of Hlin's, and the bridge is its own versioned
  public contract.
- *`SUPPORTED_BRIDGE_MAJORS` moved* from `hlin_manifest::manifest` (behind the
  `contract` feature) to a new ungated `hlin_manifest::bridge`, re-exported
  from both places, so `hlin-bridge` reuses it with `default-features = false`
  instead of duplicating it.
- *Bytes beside the JSON.* Every bytes field (`fetch.body`, `response.body`,
  `chunk.body`, `state.blob`, `init.restored`) is `Option<Vec<u8>>` with
  `#[serde(skip)]`; the `js` layer carries it as an `ArrayBuffer`.
  `Message::take_bytes` lets a sender move bytes without a copy.
- *`re` for streams is in `data`* (`chunk`, `end`, `pull`, `cancel`), as the
  specification writes them; `response` carries the fetch's id in the
  envelope's `re`. Replies the SDK makes (`heartbeat` echo, `state`) also set
  the envelope's `re` to the message they answer.
- *Type names are the specification's*: `set-param` and `set-range` on the
  wire, `set_param`/`set_range` in Rust.
- *Unknown enum values* (`refusal`, `end.error`, `method`) decode to an
  `Unrecognised`/`Other` variant instead of failing the whole message, so a
  newer minor's refusal code is still a refusal, and an `OPTIONS` fetch still
  reaches the page to be refused with `method`.
- *Keys* are ULIDs from `crypto.getRandomValues` and the clock, minted per
  `Attempt`; `Attempt::retry` resends with the same key.
- *Credit*: 256 KiB up front after the streaming `response`, then as many
  bytes as each chunk the module takes (granted on consumption, not arrival).
  `cancel` only if the stream is still open when the reader is dropped.
- *Before `init`* the SDK acts only on `init`, `heartbeat` and replies;
  `ready()` called early is sent when `init` arrives. A second `init` and any
  message of another major are ignored.
- The SDK does not enforce `request_bytes`/`fetches_in_flight`/`state_bytes`
  itself; the page does, and the refusals come back as `Reply::Refused`.

**For HLIN-T-0066.** The page should use `Envelope::<ModuleMessage>` +
`hlin_bridge::js::from_js` to read and `to_js` (passing the returned transfer
array to `postMessage`) to send, `IdMint::shell()` for ids, and
`ALLOWED_REQUEST_HEADERS` to filter. One hazard worth a look: `init` is sent
"after the frame loads", but a Trunk-built module instantiates its wasm
asynchronously, so the frame's `load` event can fire before `connect()` has
installed its listener and the `init` would be lost. The page may need to
resend `init` until `ready` or the first heartbeat echo (the SDK ignores a
second `init`, so resending is safe).

**Left.** "Used by [[HLIN-T-0071]]'s sample module" is for that task.
