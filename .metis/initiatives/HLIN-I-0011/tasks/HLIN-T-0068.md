---
id: stream-a-response-to-a-module-as
level: task
title: "Stream a response to a module, as fast as it asks and no faster"
short_code: "HLIN-T-0068"
created_at: 2026-09-25T00:01:06.693811+00:00
updated_at: 2026-09-25T11:19:04.182509+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Stream a response to a module, as fast as it asks and no faster

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 8 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Streaming* and REQ-3.6.

## Acceptance Criteria

## Acceptance Criteria

- [x] `fetch` with `stream: true` on a read yields `response`
      (`streaming: true`), then ordered `chunk`s, then `end`; on a write it is
      refused with `method`
- [x] Credit: the page reads only as many bytes as the module has `pull`ed;
      a module that stops pulling stops the read, and backpressure reaches the
      platform (a test proves nothing buffers without bound)
- [x] `cancel` aborts the request and the shell drops the upstream connection
- [x] `end` errors: `idle` (`stream_idle_seconds`), `rate`
      (`stream_bytes_per_second`), `unreachable`, `cancelled`, `unmounted`
- [x] Caps: `streams` per frame; page-wide 4 under HTTP/1.1 and 32 under
      HTTP/2+, read from the navigation's `nextHopProtocol`; past either,
      `too_many`
- [x] At `/p/`: streamed responses pass through as they arrive; the upstream
      timeout applies until headers only; `response_bytes` does not apply
- [x] The operator guide says to serve the shell over HTTP/2 for streaming
      modules
- [x] Tests, including a browser test with a server-sent event source on the
      sample platform
- [x] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0065]] and [[HLIN-T-0066]]. The SDK side is in
  [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-25 (implemented)

**What.**

- *Proxy* (`crates/hlin/src/modules/requests.rs`). The page asks for a stream
  with `X-Hlin-Stream`; a write asking for one is `method` 405 at step 4.
  Step 8 branches from `deliver_whole` to `deliver_streamed`: status and the
  allowed headers at once, plus `X-Hlin-Stream: framed`, and the body pulled
  from the platform only as hyper takes it, so backpressure runs end to end.
  The upstream timeout covers only `send()` (headers) via
  `tokio::time::timeout`, with the request's own timeout lifted to
  `Duration::MAX` (reqwest's only way to lift a client timeout; tokio caps it
  at its far future), so `AppState` and `Clients` are unchanged.
  `stream_idle_seconds` is timed only while the page is waiting (a
  `tokio::time::timeout` round each `next`), and `stream_bytes_per_second`
  is a bucket of one second's allowance; either ends the stream and drops the
  platform's body, and so its connection.
- *Framing* (`crates/hlin-stream/src/streamed.rs`). An HTTP body cannot say
  why it stopped, and `end` must, so the streamed body from `/p/` is framed:
  kind byte, u32 length, bytes; data frames as they arrive and an end frame
  (`""`, `idle`, `rate`, `unreachable`). No end frame means cut between:
  `unreachable`. Recorded in HLIN-S-0007 *Streaming*.
- *Page* (`crates/hlin-ui/src/frame.rs`, pure parts in `bridge.rs`).
  `admit_fetch` refuses `stream` on a write with `method`, then past the
  frame's `streams` or the page cap with `too_many`, and counts a stream in
  `fetches_in_flight` for its life. A stream is a `Flow` in the page, counted
  per mounting (`serial`) and page-wide; the cap is 4, or 32 when the
  navigation's `nextHopProtocol` is `h2`/`h3` (`bridge::page_stream_cap`;
  unknown is 4). `bridge::Outbox` holds what was read and sends `chunk`s only
  within credit, splitting where credit runs out, and the page reads again
  only when it holds nothing and has credit, so it holds at most one read.
  `pull` (exempt from the rate) grants credit; `cancel` and unmount (the
  panel leaving, being given up on, and the budget's `detach`) go through
  `end_flow`: `end` once, the request aborted, logged on the console. Unmount
  ends a frame's streams before the frame leaves the document.
- *Sample platform.* `/api/module/feed` (SSE ticks at a named pace and
  weight) and `/api/module/feed/{id}` (ticks and bytes written, and whether
  the connection is open), both behind identity under the module read
  prefix. The probe (still plain JS) gained `window.probeStreams` and a
  "Follow the feed" button.
- *Operator guide.* README *Modules that stream: serve the shell over
  HTTP/2*, with the stream limits.

**Tests.** `hlin-stream`: 6 framing tests. `hlin` unit: 2 bucket tests.
`crates/hlin/tests/requests.rs`: 10 new (passthrough before the platform has
finished and past the upstream timeout, `response_bytes` not applied, a
platform's 403 streams, a shell refusal stays whole, streamed write `method`,
headers-too-late `timeout`, `idle`, `rate`, the page going away drops the
platform's connection, a platform breaking off is `unreachable`).
`hlin-ui` bridge: 6 (cap, admission, end reasons, credit). Sample platform:
the feed's order and counts. `e2e/tests/streams.spec.js`, 6 browser tests:
order and end, and a streamed write refused; a module that stops pulling
gets exactly its credit (4096 bytes) while the platform stalls (measured at
about 3.8 MB written, then nothing) and flows again on `pull`; `cancel` →
`end cancelled` and the feed sees its connection close; per-frame
`too_many`; page-wide `too_many` across three frames (HTTP/1.1, skipped
otherwise); unmounting → `ended (unmounted)` on the console and the feed
closed.

**Decisions.**

- *The shell enforces idle and rate; the page reports them.* The shell holds
  the platform's connection, and the framing carries the reason to the page.
  The page enforces credit and the caps.
- *Rate ends a stream rather than slowing it*, because the specification
  names `rate` as an end. A bucket of one second's allowance, so a stream
  resuming after its module paused may burst about what the connections had
  buffered, and no more.
- *`HEAD` may be streamed* (it is a read); it ends at once.
- *Duplicate `fetch` ids* in one mounting: `pull` and `cancel` go to the
  open stream of that id.
- *No change to the SDK*: it already matches (credit on consumption, cancel
  on drop).

**Checks.** `angreal check all` clean; `angreal test all` 742 passed, 0
failed, 3 ignored; `angreal ui build` ok. Against `demo up --with aurora`,
`angreal e2e test`: 45 passed, 15 skipped, 0 failed. Against `--with collab`,
`angreal e2e signin`: 9 passed.
