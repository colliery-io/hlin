---
id: aggregator-and-stream
level: task
title: "Aggregator and stream"
short_code: "HLIN-T-0012"
created_at: 2026-09-07T14:04:21.675573+00:00
updated_at: 2026-09-07T16:15:48.044062+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Aggregator and stream

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Implement the aggregator and the stream, per [[HLIN-S-0003]]: the component that fans out to platforms on a viewer's behalf, decides every panel's state ([[HLIN-A-0001]]), and delivers one stream per surface to the browser. This is where the vision's "eight panels from six platforms respond to one picker" becomes a thing that runs.

## Acceptance Criteria

## Acceptance Criteria

- [x] `GET /api/stream/{surface_id}` as server-sent events and `POST /api/stream/{surface_id}/params`, with the frame and request types in `hlin::stream::frame`, and tests parsing the specification's own examples
- [x] A surface holds per-instance state and a generation that every parameter change carries; a frame from a superseded generation is never emitted, pinned by a test that races a slow old answer against a new range
- [x] Every request asks the platform's `Credentialer` for headers; the aggregator never names a strategy. The `Viewer`, including what `forward-session` needs, is captured when the stream opens and dropped with it
- [x] Deduplication on platform, endpoint and canonical query, within one surface and therefore one principal by construction. Two instances sharing an endpoint produce one request and two frames
- [x] Coalescing: five changes inside the settle interval produce one fan-out for the latest generation
- [x] The whole outcome table, one test per row, including 401 and 404 as malformed rather than forbidden or unknown, and registry-driven states set without a fetch
- [x] Refresh on the interval; backoff per platform from 30s to a 300s ceiling; a 15s keep-alive on the stream; a `surface` acknowledgement frame
- [x] The `detail` is written by the shell from the cause, with a test asserting not even the endpoint reaches a viewer
- [x] Current state of every instance sent on subscribe, so a reconnecting browser needs no replay
- [x] 28 tests against a controlled clock with no sockets, plus live verification against two running platforms
- [ ] **Jitter on the retry backoff is not implemented.** The specification asks for it and the doubling is currently exact. It matters when many shells retry one platform together, which a single demo shell cannot exhibit; noted here rather than silently dropped

## Implementation Notes

### Technical Approach
Separate the state machine from I/O: a pure `Aggregator` core that takes events (outcome arrived, tick, params changed, registry changed) and emits frames, driven by a thin async shell that does the fetching. That makes the outcome table testable without a network and the fake clock straightforward. Use `tokio::sync::broadcast` or a per-surface channel for frame delivery; the SSE handler is a consumer.

### Dependencies
[[HLIN-T-0011]] for the registry's current view and the `Issuer`; [[HLIN-T-0010]] for minting; the store for subscriptions.

### Risk Considerations
Generation handling has to be right on both sides or the frontend renders stale data with the wrong time range. Pin it with a test that races a slow old-generation response against a fast new one.

## Status Updates

**2026-09-07 — complete, with one gap named below.** The stream runs. `hlin::stream` in four parts: `frame` (the wire types), `aggregator` (the state machine, with no I/O in it), `live` (the driver that fetches), and `surfaces` (what is currently being served). 28 aggregator tests; workspace at 201; `angreal check all` clean.

Verified against two running platforms: opening the stream delivers frames for all twelve panels, every one reaching `ready`; posting a parameter change is acknowledged, advances the generation, and refetches everything.

**The separation the task asked for paid for itself immediately.** The state machine takes events and emits frames with no network in it, so all 28 tests run against a clock the test controls, with no sockets and no sleeping. Every row of the outcome table, the backoff growth and ceiling, the coalescing and the generation race are ordinary unit tests as a result. All 28 passed on the first run, which I do not think would have been true had the fetching been tangled into them.

**A real bug, found only by running it.** With both platforms on `hlin-token`, one platform served every panel and the other refused every one, with `identity names key …, which the issuer does not publish`. The cause was in `hlin-identity`, not here: a platform meeting its first burst of requests has a cold key cache in every one of them concurrently. The first triggered a fetch; the rest hit the once-a-minute rate limit, found no key, and refused perfectly good tokens. The fix serialises the fetch under the write lock, so concurrent callers wait for the answer the first is already getting, and exempts a genuinely cold cache from the rate limit, since a platform that starts before the shell would otherwise never recover. `a_burst_of_first_requests_all_succeed` keeps it fixed. That bug was invisible to 20 existing tests because none of them was concurrent.

Decisions taken during the work:

- **Deduplication keys on platform, endpoint and query, not principal.** A surface belongs to one viewer, so every request it makes is already for one person; adding the principal to the key would be noise implying a sharing that cannot happen. The comment says so, because the omission looks like the bug decision HLIN-A-0004 warns against and is the opposite of it.
- **A panel holding data does not go back to loading.** A chart that blanked on every refresh would be unusable, so only a panel with nothing to show says it is loading.
- **Any answer at all clears the backoff, even a refusal.** A 403 proves the platform is there; only silence earns a wait.
- **Staleness is checked once a second**, not on every 100ms tick. There is nothing to gain from noticing it ten times faster.

**Not done, and not quietly:** jitter on the retry backoff. The specification asks for it; the doubling is exact. It matters when many shell instances retry one platform in step, which one demo shell cannot exhibit, so it is left as a named gap rather than a silent omission.

Note for [[HLIN-T-0014]]: the surface `all` currently means every panel every platform offers, which is enough to exercise the stream and is replaced by real layouts in [[HLIN-T-0015]]. The frontend should read `hlin::stream::frame` types and drop frames from a superseded generation on its side too.
