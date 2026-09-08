---
id: 001-aggregator-owns-the-panel-state
level: adr
title: "Aggregator owns the panel state machine"
number: 1
short_code: "HLIN-A-0001"
created_at: 2026-08-04T14:19:25.581881+00:00
updated_at: 2026-08-04T14:20:00.004577+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-1: Aggregator owns the panel state machine

## Context

The vision ([[HLIN-V-0001]]) requires rendering to be total over its inputs: every panel is in exactly one of loading, ready, stale, or unavailable, with unavailability distinguishing unreachable from malformed from unknown from deprecated. The system decomposition in [[HLIN-I-0001]] splits responsibility between a shell-side aggregator (fan-out, dedup, one stream to the browser) and a browser-side renderer (envelope + view kind → design-system component).

Something has to decide which state each panel is in, and the choice shapes the envelope design, which is upstream of the vocabulary and view registry work. Deciding it before the manifest schema keeps the envelope contract from being designed around an unowned state machine.

## Decision

The aggregator owns the panel state machine. Panel state is determined shell-side and carried explicitly in the stream protocol. The renderer draws the state it is told.

Envelopes are pure data for a view kind. State, staleness, and failure classification travel alongside the envelope in the stream frame, not inside it.

**The one exception (amended 2026-09-07):** when the browser loses its stream to the shell, no server exists to tell it anything, and rendering must stay total. The renderer therefore derives exactly one transition locally: on stream loss every panel becomes `stale`, and after a shell-configured grace interval `unavailable (unreachable)`, where the unreachable party is the shell itself. This is the only client-side state transition; it exists because the aggregator cannot report its own absence.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Aggregator owns state | State decided where the network facts are; envelopes stay pure data; renderer is trivially total (draws what arrives) | Stream protocol becomes a second versioned contract alongside the manifest | Low | Low |
| Renderer derives state | Stream protocol stays thin (envelopes only) | Staleness clocks and failure classification move into the client; every envelope must carry enough metadata to reconstruct state; unreachable-vs-malformed distinction is faked client-side, since only the shell observed the failure | Medium | Medium |
| Split: aggregator owns unavailability, renderer owns staleness | Each contract smaller | State machine no longer total in one place; two components must agree to cover all states; boundary cases (stale-then-unreachable) need cross-component rules | High | Medium |

## Rationale

The aggregator is the component that actually observes the facts the state machine classifies. It made the request, so it knows unreachable from malformed; it holds the manifest, so it knows unknown from deprecated; it knows when data was last fresh, so it decides stale. Deriving state anywhere else means reconstructing those observations secondhand.

Totality is the vision's hard requirement, and totality is easiest to guarantee when one component owns the whole machine and the renderer's job reduces to a total function from (state, envelope, view kind) to a component.

## Consequences

### Positive
- Envelope design (next in this initiative) is unconstrained by state concerns: an envelope is data for a kind, nothing else.
- The renderer needs no clocks, no error taxonomy, and no network awareness — it becomes straightforwardly testable as a pure function.
- The unreachable/malformed/unknown/deprecated taxonomy lives in one place, so extending it is a single-component change.

### Negative
- The stream protocol is now a versioned contract of its own: frame = panel id + state + (envelope when present). It must be designed and evolved with the same additive discipline as the manifest.
- The aggregator becomes load-bearing for correctness, not just efficiency; its state transitions need explicit tests against the totality requirement.

### Neutral
- Staleness thresholds become shell-side configuration rather than a client concern.
