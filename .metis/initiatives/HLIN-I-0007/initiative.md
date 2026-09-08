---
id: platforms-tell-the-shell-when-data
level: initiative
title: "Platforms tell the shell when data changed"
short_code: "HLIN-I-0007"
created_at: 2026-09-08T10:51:52.562014+00:00
updated_at: 2026-09-08T10:51:52.562014+00:00
parent: hlin
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/design"

exit_criteria_met: false
estimated_complexity: M
initiative_id: platforms-tell-the-shell-when-data
---

# Platforms tell the shell when data changed

## Context

The shell polls. Measured on the demo: 20 upstream requests a second for one
surface of four panels at `refresh_ms = 125`, independent of how many people are
watching, paid continuously whether or not anything changed. [[HLIN-A-0010]]
records why that design is right and what it costs; [[HLIN-A-0011]] decides how
to remove the waste without giving up what it buys; [[HLIN-S-0006]] says what
that looks like in detail.

The shape, in one line: a platform declares an event stream, the shell
subscribes to it, an event says *that* a panel changed rather than *what* it
changed to, and polling continues underneath at a relaxed interval so that
nothing about the feature being absent, broken, or lying can make the shell
worse than it is today.

## Goals

- A platform can tell the shell its data changed, and the shell fetches sooner.
- A platform that says nothing is completely unaffected, in code and in load.
- Every failure of the new path degrades to the polling behaviour that exists
  now, and the suite proves it against platforms that misbehave in four
  different ways.
- The redundant-request rate on the demo drops measurably, and the measurement
  is in the repository rather than in a commit message.

## Non-Goals

- Push carrying panel data. Decided against in HLIN-A-0011; a frame is a
  property of a panel *and* its viewer, which a platform cannot know.
- Guaranteed delivery, acknowledgements, replay or ordering. The poll underneath
  is the recovery mechanism, and that is what keeps this cheap.
- Any change to the browser protocol. A browser cannot tell whether a frame was
  prompted by a timer or an event, and has nothing to do differently.
- Removing polling, now or later.

## Detailed Design

See [[HLIN-S-0006]]. The initiative's work is that specification, built.

## Implementation Plan

Decomposition is the next step and is deliberately not written yet: the
specification has three open items — the relaxed interval's default, the
`selections` matching rule, and whether stream health reaches the operator
channel — and at least the first two want a decision before tasks are cut,
because they change what a task would say.

## Risks

| Risk | Mitigation |
|------|------------|
| A platform floods events and the shell fetches itself to death | The refresh floor (HLIN-A-0009) applies to event-prompted fetches, plus a coalescing window with jitter |
| One event for a widely watched panel fires many simultaneous fetches | Same coalescing window; called out in HLIN-A-0011 so it cannot be skipped quietly |
| A platform silently stops reporting | The relaxed interval is a real interval, so staleness is bounded rather than unbounded |
| The feature becomes load-bearing and the polling floor is quietly dropped | REQ-1.1 and REQ-1.2 make "correct with no events" a conformance requirement with tests |

## Status Updates

### 2026-09-08 — Designed, not yet decomposed

Two ADRs and a specification, from the question "why are we polling instead of
using a push method?".

The two decisions were taken separately on purpose. **Who connects** is settled:
the shell subscribes to a path the platform declares, so a platform stays a
plain HTTP server that needs no address for the shell and no credential to send
it — a platform POSTing inbound would invert the dependency the whole product
rests on. **What is sent** was worked out in the design rather than assumed: the
news, not the data, because a frame is a property of a panel *and* who is asking
*and* what they selected, and the shell fans out on exactly that. A platform
could only push frames that are identical for everybody, and cannot identify
which of its panels those are.

Sending only the news has the property that made it worth choosing: nothing
downstream changes. The credentialing, the parameter allow-list, the envelope
validation, the per-principal dedup and every row of the outcome table in
[[HLIN-S-0003]] still describe the fetch, because the fetch still happens. Push
replaces the timer.

Awaiting a decision on the three open items in [[HLIN-S-0006]] before cutting
tasks.

### 2026-09-08 — All six complete

Push is built, measured, and proved against platforms that misbehave.

**The number.** One surface holding a panel that changes irregularly every few
seconds and declares `refresh_ms = 250` so it feels immediate: **156 upstream
requests a minute polled, 14 with its platform's stream connected** — and the
panel is *more* current, because a change is fetched when it happens rather than
up to a quarter-second later. Both measured on the demo, recorded in
[[HLIN-S-0006]].

**What the work taught that the design did not.**

*Push is worth nothing for fast-moving data.* Building the sample platform made
this concrete: the two panels whose data genuinely moves eight times a second
gain nothing from being reported on, and do not claim to be. The feature earns
its keep exactly where polling forces a choice between being late and being
wasteful.

*Three defects only appeared by measuring or by writing the hostile test.* The
shell's shared client killed every subscription after ten seconds, because a
request timeout covers the body and a held-open response is all body — and the
first measurement read as a modest saving rather than as total failure. A
connected stream had to announce itself rather than be inferred from a first
event, or a platform reporting perfectly and having nothing to say would never
relax anything. And a full channel blocked the reader, which would have made the
shell's consumption rate into the platform's problem; it drops now, which is
safe for the same reason the whole feature is safe.

*A test caught a metronome pretending to be a hash*, and another caught a parser
that discarded every well-formed event because a blank line has no colon. The
second had a correct test written for it that I had simply not run.

367 Rust tests (from 333 at the start of this initiative), 24 browser tests, the
walkthrough holds, clippy clean.

Ready for review.
