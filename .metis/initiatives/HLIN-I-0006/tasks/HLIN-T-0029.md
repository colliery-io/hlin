---
id: cap-upstream-response-bodies
level: task
title: "Cap upstream response bodies before allocating them"
short_code: "HLIN-T-0029"
created_at: 2026-09-08T01:56:09.594676+00:00
updated_at: 2026-09-08T03:03:47.749743+00:00
parent: HLIN-I-0006
blocked_by: []
archived: false

tags:
  - "#task"
  - "#tech-debt"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0006
---

# Cap upstream response bodies before allocating them

## Objective

HLIN-S-0002 makes any envelope over 1 MiB malformed. The check is correct and
runs on a buffer that has already been allocated to whatever length the platform
sent. Neither `reqwest::Client` has a body limit.

Finding 4 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P1 - High

### Technical Debt Impact
- **Current Problems**: `stream/live.rs:253` `response.bytes().await`, then
  `envelope_validate.rs:136` checks `bytes.len()`. `options.rs:114`
  `answer.text().await` has the same shape, and so does the manifest client.
- **Benefits of Fixing**: A platform having a bad day costs the shell one
  refused body rather than its memory.
- **Risk Assessment**: The shell exists to degrade when a platform misbehaves.
  This is the one place a misbehaving platform takes the shell down with it,
  once per panel per refresh.

## Acceptance Criteria

- [x] Refuse on `Content-Length` over the limit before reading a byte
- [x] Read with `Response::chunk()` against a running total; abandon at the limit
- [x] Return `Outcome::Malformed` carrying the existing `TooLarge` defect, so the
      panel says the right thing
- [x] Same guard in `options.rs` and `manifest_client.rs`
- [x] Test with a body one byte over the limit

## Implementation Notes

### Technical Approach
One helper — `read_bounded(response, limit) -> Result<Bytes, TooLarge>` — used
in all three places, so the limit cannot be enforced in one and forgotten in
another.

### Dependencies
None.

## Status Updates

### 2026-09-08 — bounded before allocating, in all three places

`crate::bounded::read_bounded(response, limit)` is the one helper, used by the
panel fetch, the options route and the manifest client — so the limit cannot be
enforced in one and forgotten in another, which was half the reason there were
three copies of the same mistake.

**Two guards, because either alone has a hole.** `Content-Length` is refused
before a byte is read, which costs nothing and covers the honest case. Then the
body is read in chunks against a running total, because that header is optional,
absent under chunked transfer encoding, and in any case a claim by the same
party whose length is in question.

**Each caller maps the refusal to the state that tells the truth:**

- Panel fetch → `Outcome::Malformed`. The platform answered, and what it
  answered is the problem, which is what an operator needs to be told.
  `Unreachable` would send them looking at the network.
- Manifest client → `Fetched::Unreadable`, for the same reason.
- Options route → an empty list, which is what every other failure there does;
  the control falls back to a typed value and a person loses convenience only.

**Four tests, driving a real socket**, because the behaviour is about how a body
arrives and none of that survives being faked:

- a body exactly at the limit is read whole
- one byte over is refused
- a *chunked* body that declares no length is still refused, and gives up within
  one chunk of the limit rather than reading all 4 KiB of it — the case
  `Content-Length` alone cannot catch, and the one any streaming handler
  produces without meaning anything by it
- a declared length is refused before the body is read, reporting what was
  claimed

291 Rust tests pass, 0 failures. `angreal check all` clean. The walkthrough
holds.

Required a workspace change: `reqwest`'s `stream` feature, for `bytes_stream`.

### The browser suite

15 of 22, with `surface.spec.js` failing at "resizing" and its remaining tests
skipped (that file runs `mode: 'serial'`). This is [[HLIN-T-0041]] again — the
same cross-file failure, now landing on the test where it was originally
recorded in [[HLIN-I-0004]] rather than on "removing a panel". It moves between
the two.

Not evidence about this change: the Rust suite, including the four new
socket-driven tests, is what covers it, and the walkthrough passes. But the
browser suite is now unreliable enough to be blocking, so [[HLIN-T-0041]] is
next rather than later in the sequence.
