---
id: stream-protocol-specification
level: task
title: "Stream protocol specification"
short_code: "HLIN-T-0005"
created_at: 2026-09-07T12:13:57.755147+00:00
updated_at: 2026-09-07T13:12:45.072807+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# Stream protocol specification

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Write the stream-protocol specification, the second versioned contract named in [[HLIN-A-0001]]: how the shell delivers panel state and envelopes to the browser over one stream, and the aggregator policies the other specs deferred to it.

## Acceptance Criteria

## Acceptance Criteria

- [x] A Metis specification under this initiative covering: subscription model (a layout's panel instances plus current parameter values), frame shape (panel instance id, state, envelope when present, `as_of`, age), protocol versioning (additive, unknown fields ignored, same discipline as the manifest) — [[HLIN-S-0003]]
- [x] Transport decided and recorded: server-sent events shell→browser, plain `POST` browser→shell. Recorded in the specification with its reasoning rather than as an ADR, since nothing about it was contentious
- [x] Default staleness window, stream-loss grace interval, and the client-side transition from [[HLIN-A-0001]]'s amendment all specified with defaults, in one table with every other interval
- [x] Per-principal dedup ([[HLIN-A-0004]]) and coalescing policy specified, including generations, which are what make a superseded fan-out unambiguous
- [x] Every `PanelState` from [[HLIN-S-0002]] appears in the frame vocabulary; a full outcome-to-state mapping including timeout, 403, parse failure, over-limit and the registry-driven states that need no fetch
- [x] How contract violations and operator signals ([[HLIN-A-0002]]) surface: a separate operator channel the viewer's stream never carries
- [x] Worked example: three panels from two platforms, through a time-range change, a platform going down, a violating redeploy, and a dropped stream

## Implementation Notes

### Technical Approach
Specification only; no code. Written to be encoded later as types in `hlin-manifest` or a small `hlin-stream` module inside `hlin`.

### Dependencies
[[HLIN-S-0002]] (states), [[HLIN-A-0004]] (dedup key). No code dependency.

### Risk Considerations
Scope creep into the layout engine's editing protocol. This spec is read-path only; layout editing is ordinary request/response.

## Status Updates

**2026-09-07 — complete.** [[HLIN-S-0003]] written. Specification only, as scoped; no code.

Decisions taken while writing it:

- **Server-sent events, with control changes as ordinary `POST`s.** The traffic is almost entirely one-directional: the shell pushes continuously, the browser speaks rarely and in discrete events. Server-sent events give reconnection, event ids and plain HTTP semantics for nothing, and the upstream direction being ordinary requests means a control change is an ordinary thing to authorise, log and retry. Recorded in the specification with its reasoning rather than as an ADR, since nothing about it was contentious. The revisit condition is written down: a surface changing at very high frequency would want binary framing, and the protocol version is how that arrives.
- **Generations.** A number per surface, incremented on every parameter change, carried on every request and every frame. It is what stops a slow panel from a previous time range painting over a newer one, and it makes "supersede rather than race" implementable rather than aspirational. This was not in the task's criteria; the coalescing requirement is not satisfiable without something like it.
- **A 404 is `malformed`, not `unknown`.** The manifest says the panel exists, so a 404 means the platform and its own manifest disagree. That is a defect. `unknown` is reserved for the registry no longer listing the panel, which is a different thing that needs a different answer from the viewer.
- **Retry backs off per platform, not per panel.** A platform that is down should not be hammered once per panel by every viewer.
- **The viewer's reason is written by the shell**, never passed through from a platform. A platform's error body may contain anything, and the shell does not put text a platform wrote in front of a person.
- **Interval defaults are marked as a starting point** rather than a considered claim. They want a real deployment before anyone trusts them.

On the risk this task recorded, scope creep into layout editing: the specification says in its overview that it is the read path only, and layout editing is left to the layout engine as ordinary request and response.

Four open items are recorded in the specification, the sharpest being whether generations should be per surface or per panel instance. Per surface is simpler and matches a global time picker, but a single panel's own selection changing currently bumps the whole surface and refetches more than it needs.
