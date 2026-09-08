---
id: stream-protocol
level: specification
title: "Stream Protocol"
short_code: "HLIN-S-0003"
created_at: 2026-09-07T13:10:42.623625+00:00
updated_at: 2026-09-07T13:10:42.623625+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Stream Protocol

## Overview

Hlin's second versioned contract. Where the manifest ([[HLIN-S-0001]]) is what a platform tells the shell, the stream is what the shell tells a browser: for every panel on a surface, what state it is in and what data it has.

The stream exists because of one decision. The aggregator owns the panel state machine ([[HLIN-A-0001]]), so state must travel; and it travels *alongside* the envelope rather than inside it, which is what keeps envelopes pure data. That makes the frame, not the envelope, the unit of this protocol.

It also exists because of one promise in the vision: eight panels from six platforms respond to one time picker without any of them knowing about each other. That fan-out is aggregated shell-side, deduplicated, and delivered over a single stream. This specification says how.

Scope is the **read path** only. Editing a layout, publishing it, or forking it is ordinary request and response, and belongs to the layout engine.

**Refinement, 2026-09-08.** A platform may now tell the shell that a panel's data changed ([[HLIN-A-0011]], [[HLIN-S-0006]]). Nothing in this document changes as a result, and that is the point of how it was designed: an event moves a fetch earlier and carries no data, so every frame a browser receives is still produced by a fetch the shell issued, and every row of the outcome table below still describes it. The only observable difference is that a frame may arrive sooner than the cadence would have produced it.

## System Context

### Actors
- **Browser**: opens one stream per surface, sends control changes as ordinary requests, renders each frame through `hlin-view`.
- **Aggregator (shell)**: owns subscriptions, fans out to platforms with forwarded identity ([[HLIN-A-0004]]), deduplicates per principal, decides state, emits frames.
- **Registry (shell)**: tells the aggregator when a platform's contract changed, so panels can become unknown or deprecated without a fetch.
- **Platforms**: answer data requests. They know nothing about the stream.

### Boundaries
Inside: the subscription model, frame shape, protocol versioning, state derivation, coalescing, and the operator channel. Outside: the envelope shapes ([[HLIN-S-0002]]), the manifest ([[HLIN-S-0001]]), the identity token ([[HLIN-T-0006]]), and layout editing.

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | One surface is served by one stream, carrying every panel instance on it | The single-stream promise; a browser holding twelve connections is what this design exists to avoid |
| REQ-1.2 | Every frame names a panel instance and a state; an envelope is present only where the state shows data | State travels beside the envelope, never inside it ([[HLIN-A-0001]]) |
| REQ-1.3 | The protocol version is monotonic and additive; unknown fields and unknown frame types are ignored, never fatal | The same discipline as the manifest; a browser and a shell of different vintages must interoperate |
| REQ-1.4 | Every state in [[HLIN-S-0002]] appears in the frame vocabulary, and every fetch outcome maps to exactly one | Rendering is total over its inputs, which requires the stream to be total over outcomes |
| REQ-2.1 | Requests are deduplicated on `(platform, endpoint, canonical params, principal)`; one upstream request serves every panel instance that asked for it | The fan-out promise, made safe by keying on principal ([[HLIN-A-0004]]) |
| REQ-2.2 | A control change coalesces: the shell waits a settle interval before fetching, and supersedes an in-flight generation rather than racing it | Dragging a time picker must not fan out once per mouse move |
| REQ-3.1 | Contract violations and manifest defects reach operators on a channel the viewer's stream never carries | A platform's versioning mistake is not the viewer's problem ([[HLIN-A-0002]]) |
| REQ-3.2 | On stream loss the browser derives `stale`, then `unavailable (unreachable)` after a grace interval | The one client-side transition, because the shell cannot report its own absence ([[HLIN-A-0001]] amendment) |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | A reconnecting browser reaches a correct view without the shell replaying history | Streams drop; correctness must not depend on having seen every frame |
| NFR-1.2 | Frames for one panel instance are ordered; frames for different instances need no ordering between them | Panels are independent, and ordering across all of them would serialise the fan-out |

## Transport

**Server-sent events** from shell to browser, over the single-origin session the routing layer already provides. Control changes go the other way as ordinary `POST` requests.

Chosen over WebSocket because the traffic is almost entirely one-directional: the shell pushes frames continuously, the browser speaks rarely and in discrete events. Server-sent events give reconnection, event ids and ordinary HTTP semantics for free, and cost nothing that this shape of traffic needs. The upstream direction being plain requests also means a control change is an ordinary thing to authorise, log, and retry.

```
GET  <shell>/api/stream/{surface_id}        text/event-stream
POST <shell>/api/stream/{surface_id}/params application/json
```

A revisit if it ever bites: a surface with many panels changing at very high frequency would be better served by a binary framing, and the protocol version is how that would arrive.

## Subscription

A stream is opened for one surface. The shell resolves the surface to its panel instances from the layout store, and holds, per instance:

| Field | Meaning |
|---|---|
| `instance` | The panel instance id from the layout; unique within the surface |
| `panel` | `platform_id/key`, the manifest reference |
| `params` | The parameter values in force: the surface's time range, plus this instance's own selections |
| `generation` | Which round of parameters this instance is on |

**Generations** are how a control change is made unambiguous. Every parameter change increments the surface's generation; every request and every frame carries it; a frame from a superseded generation is dropped by the browser. This is what stops a slow panel from a previous time range painting over a newer one.

Sending parameters:

```json
POST /api/stream/{surface_id}/params
{ "protocol_version": 1,
  "generation": 7,
  "time_range": { "from": "2026-09-07T10:00:00Z", "to": "2026-09-07T11:00:00Z" },
  "selections": { "panel-3": { "cluster": ["us-east"] } } }
```

The shell answers `202` and the effects arrive as frames. A generation lower than the one in force is ignored, so an out-of-order control message cannot rewind a surface.

## Frames

Every frame is one JSON object in one server-sent event. `event:` names the frame type so a browser can ignore a type it does not know (REQ-1.3).

### `panel` — the state of one panel instance

```json
{
  "protocol_version": 1,
  "instance": "panel-3",
  "generation": 7,
  "state": "ready",
  "as_of": "2026-09-07T11:30:00Z",
  "age_seconds": 4,
  "envelope": { "envelope": "scalar.v1", "value": 1523, "unit": "per_second" }
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `protocol_version` | integer ≥ 1 | yes | Monotonic, additive |
| `instance` | string | yes | Panel instance on this surface |
| `generation` | integer | yes | Parameters this frame answers |
| `state` | see below | yes | |
| `as_of` | RFC 3339 | no | When the data was true, from the envelope where it said so, else fetch time |
| `age_seconds` | integer | no | How old the data is, so the browser need not hold a clock |
| `envelope` | envelope document | no | Present only where the state shows data |
| `cause` | see below | no | Required when `state` is `unavailable` |
| `detail` | string | no | One line for the viewer. Never a stack trace, never a platform's error body |
| `successor` | string | no | The panel key that replaces this one, where the manifest named one |

`state` is `loading`, `ready`, `stale`, or `unavailable`. When `unavailable`, `cause` is one of `unreachable`, `malformed`, `unknown`, `deprecated`, `forbidden`, matching [[HLIN-S-0002]] exactly.

### `surface` — something changed about the surface as a whole

```json
{ "protocol_version": 1, "generation": 7, "acknowledged": true }
```

Sent when a generation takes effect, so a browser can show that a control change was received before any panel has answered.

### `heartbeat`

An empty comment frame every 15 seconds. It keeps intermediaries from closing an idle stream and gives the browser a liveness signal that does not depend on any panel changing.

## Deriving state

The aggregator maps each outcome to exactly one state (REQ-1.4):

| Outcome | State |
|---|---|
| Request in flight, nothing previously received for this generation | `loading` |
| 2xx, envelope parses and matches what the panel promised | `ready` |
| Last good envelope older than the staleness window | `stale` |
| Connection refused, reset, timeout, or 5xx | `unavailable (unreachable)`, last envelope retained |
| 403 | `unavailable (forbidden)` |
| 2xx but the envelope is unparseable, mismatched, or over a limit | `unavailable (malformed)` |
| 4xx other than 403 | `unavailable (malformed)` |
| Panel declaration invalid, or kind/envelope pairing not allowed | `unavailable (malformed)`, without a fetch |
| Panel or platform no longer in the registry | `unavailable (unknown)`, without a fetch |
| Past the panel's sunset | `unavailable (deprecated)`, without a fetch |

A 404 is deliberately `malformed` rather than `unknown`: the manifest says the panel exists, so the platform and its manifest disagree, and that is a defect rather than a removal. `unknown` is reserved for the registry no longer listing it.

### Intervals

| Setting | Default | Notes |
|---|---|---|
| Staleness window | 3 × the panel's refresh interval, minimum 60s | When data becomes visibly aged |
| Refresh interval | 30s | How often a subscribed panel is refetched |
| Stream-loss grace | 30s | Browser-side: `stale` immediately on loss, `unreachable` after this |
| Settle interval | 250ms | How long a control change waits before fanning out |
| Heartbeat | 15s | |
| Upstream timeout | 10s | Then `unreachable` |

All are shell configuration. The defaults are a starting point to be revised against a real deployment rather than a considered claim about the right numbers.

## Fan-out, deduplication and coalescing

**Deduplication** keys on `(platform, endpoint, canonical params, principal)`. Two panel instances asking for the same thing produce one upstream request and two frames. Canonical params are the encoded query in sorted order, so two instances that differ only in how their parameters were written still share.

The principal is in the key because a platform may legitimately answer two people differently ([[HLIN-A-0004]]). Sharing across principals would be a correctness bug, not an optimisation.

**Coalescing** handles the picker being dragged. A parameter change starts the settle interval; further changes within it replace the pending one and restart it; when it elapses, one fan-out goes out for the latest generation. In-flight requests from a superseded generation are abandoned rather than awaited, and any frame they would have produced is dropped.

**Retry** for `unreachable` is exponential from the refresh interval to a ceiling of five minutes, with jitter, per platform rather than per panel. A platform that is down should not be hammered once per panel by every viewer.

## The operator channel

Contract violations ([[HLIN-A-0002]]), manifest defects, and newer-schema notices never reach a viewer's stream (REQ-3.1). A viewer sees a panel that is unavailable, with one line of reason. An operator sees which platform, which panels, the diff, and the version bump that was expected.

Operator signals go to the shell's ordinary logging and metrics, and are readable through a separate authenticated endpoint. That endpoint is a shell surface like any other and is out of scope here.

The reason a viewer sees is written by the shell from the structured defect, never passed through from a platform. A platform's error body may contain anything, and the shell does not put anything a platform wrote in front of a person.

## Worked example

A surface with three panels: two from `orebank`, one from `stampmill`. Both `orebank` panels read the same endpoint.

1. **Open.** Browser opens the stream. The shell emits three `panel` frames at `loading`, generation 1.
2. **First fan-out.** Two upstream requests, not three: the two `orebank` panels deduplicate. Frames arrive as they resolve; the `stampmill` panel is `ready` first, the two `orebank` panels are `ready` from one response.
3. **Time range changes.** The picker moves four times in a second. One generation increment per change; the settle interval means one fan-out, for generation 5. A `surface` frame acknowledges it. Three `panel` frames follow at `loading`, then `ready`.
4. **A platform goes down.** `orebank` stops answering. Both its panels go `unavailable (unreachable)` and keep their last envelope, dimmed. Retry backs off per platform, so one request per interval rather than two. The `stampmill` panel is untouched.
5. **The platform returns, having shipped a breaking change without a major bump.** The registry classifies the diff as a violation, raises it on the operator channel, and applies the new contract. One `orebank` panel's key is gone, so it becomes `unavailable (unknown)` with its successor. The other returns to `ready`. Nothing about the violation reaches the browser.
6. **The stream drops.** The browser marks all three `stale` at once. After the grace interval they become `unavailable (unreachable)`, naming the shell rather than a platform. On reconnect the shell sends current state for every instance, and the browser is correct again without any replay.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0001]] | Aggregator owns the panel state machine | decided | State travels in the frame; the browser derives only the stream-loss transition |
| [[HLIN-A-0002]] | Content hash + semver, enforced at runtime | decided | Violations go to operators, never to a viewer's stream |
| [[HLIN-A-0004]] | Auth hoisted, identity forwarded | decided | Dedup keys on principal; 403 maps to `forbidden` |

## Open Items

- Whether a surface's generation should be per surface or per panel instance. Per surface is simpler and matches a global time picker; a per-instance selection changing currently bumps the whole surface, which refetches more than it needs.
- Whether `age_seconds` belongs in the frame at all, or whether the browser should compute it from `as_of`. Sending it avoids a clock-skew problem and costs a few bytes.
- Backpressure: what a shell does when a browser reads slower than panels change. Dropping superseded frames per instance is probably enough, since only the latest matters.
- Whether the operator endpoint deserves its own specification, or belongs with the registry's.
