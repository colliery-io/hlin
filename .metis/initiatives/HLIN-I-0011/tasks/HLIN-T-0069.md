---
id: an-sdk-so-a-leptos-module-does-not
level: task
title: "An SDK so a Leptos module does not know it is in a frame"
short_code: "HLIN-T-0069"
created_at: 2026-09-25T00:01:07.895454+00:00
updated_at: 2026-09-25T00:33:38.259244+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


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

- [ ] A new published-shape crate (name to settle, e.g. `hlin-module`) for
      Leptos 0.8 modules compiled to `wasm32`
- [ ] Handles the handshake: waits for `init`, exposes `ready()`, answers
      `heartbeat` automatically
- [ ] Context, theme, visibility and viewer as signals; theme tokens applied
      to the frame's document
- [ ] `fetch` as an async call returning status, headers and body, with the
      shell's refusal distinguishable from the platform's answer; writes mint
      an idempotency key per attempt and reuse it on an explicit retry
- [ ] Streams as an async reader that pulls credit as it is consumed, and
      cancels when dropped ([[HLIN-T-0068]])
- [ ] `set_param`, `set_range`, `navigate`, `changed`, `notice` as functions
- [ ] A `suspend` hook returning bytes, and the restored bytes at startup
- [ ] Message types shared with the shell page, not duplicated
- [ ] Unit tests for encoding and the handshake; used by [[HLIN-T-0071]]'s
      sample module
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Built alongside [[HLIN-T-0066]]: each is the other's test.
- Put the wire types in one crate both sides depend on (an extension of
  `hlin-stream`, or a new small crate), so the protocol cannot drift.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
