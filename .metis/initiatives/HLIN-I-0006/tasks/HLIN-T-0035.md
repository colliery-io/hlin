---
id: reconcile-a-surface-in-place-when
level: task
title: "Reconcile a surface in place when its layout is written"
short_code: "HLIN-T-0035"
created_at: 2026-09-08T01:56:17.848010+00:00
updated_at: 2026-09-08T04:44:32.326273+00:00
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

# Reconcile a surface in place when its layout is written

## Objective

`layouts.rs:320` `replace` calls `surfaces.forget(&id)`. Moving a panel one cell
writes the whole layout, drops the running surface, resubscribes, and refetches
every panel from scratch. Recorded as a known cost in [[HLIN-I-0003]]; filed
here because it compounds with [[HLIN-T-0027]].

Finding 10 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P2 - Medium

### Technical Debt Impact
- **Current Problems**: At the 30-second default every drag is a visible blink;
  the browser tests pass only because `drewSomething` polls. Every drop also
  leaves the old driver running if anything else still holds it.
- **Benefits of Fixing**: Composition stops interrupting watching.
- **Risk Assessment**: Low on its own; it makes [[HLIN-T-0027]]'s leak worse.

## Acceptance Criteria

- [x] `LiveSurface::replace_instances(new)` adds and removes instances against
      the new layout and keeps held envelopes for those that survived
- [x] A panel that only moved is never refetched
- [x] Browser test: drag a panel, assert its `data-state` never leaves `ready`

## Implementation Notes

### Technical Approach
`Instance` already carries everything needed; the diff is by instance id.

### Dependencies
[[HLIN-T-0027]] first, so a replaced surface's old driver also stops.

## Status Updates

### 2026-09-08 — a write no longer costs the surface its data

Two halves, and both were needed. `layouts::replace` calls
`Surfaces::reconcile` instead of `forget`, and the browser no longer bumps its
revision on a save — so the surface is neither dropped nor re-subscribed to.
Either alone would have left the blink: the shell would have kept the data and
the browser would have thrown it away by reconnecting, or the reverse.

**What survives is decided narrowly, and the narrowness is the design.** An
instance keeps what it fetched only when its id, endpoint, envelope *and*
retired flag are all unchanged. A panel that moved is the same panel. One whose
platform now serves it from a different endpoint, or promises a different
envelope, is not — and showing what the old one held under the new one's name
would be the shell asserting something nobody told it.

**Four tests, one per case:**

- a panel that only moved keeps its data, and nothing is due afterwards — a
  write that changed no data must not refetch
- a panel added by a write is fetched and the others are not
- a panel whose endpoint moved does *not* keep the old answer, and is asked
  afresh
- a panel removed by a write is gone, and the browser is told about what
  remains

All frames are sent, not only the changed ones: a browser has just been told the
layout was written and cannot know which panels it kept, so the honest answer is
the state of everything — which is what a fresh subscription would have given it
anyway, minus the refetch.

**And the browser test now says so.** The drag test asserts the moved panel is
still `ready` and still drawing, which is the claim in the form a person would
notice.

A layout that has been deleted or is no longer visible still falls back to
`forget`, since there is nothing to reconcile against.

303 Rust tests (up from 299), 23 browser tests, 1 honestly skipped. Walkthrough
holds. `angreal check all` clean.
