---
id: the-sample-platform-reports-its
level: task
title: "The sample platform reports its own changes"
short_code: "HLIN-T-0043"
created_at: 2026-09-08T11:03:19.026087+00:00
updated_at: 2026-09-08T11:17:13.407737+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# The sample platform reports its own changes

## Parent Initiative

[[HLIN-I-0007]]

## Objective

Teach the sample platform to serve an event stream, so there is something real
to subscribe to and the demo can show the request rate fall.

Second rather than last, because every task after this one is easier to build
and impossible to demonstrate without it.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] `hlin-sample-platform` serves SSE at `api/events`, declares it in its
      manifest, and marks its genuinely-changing panels `pushed`
- [ ] It emits a `changed` event when a panel's underlying data actually moves,
      not on a timer — a platform that emits on a timer has reinvented polling
      with extra steps
- [ ] It emits a comment-line heartbeat every 20 seconds
- [ ] It holds many concurrent subscribers without them interfering, since the
      shell reconnects and a test suite will subscribe repeatedly
- [ ] `angreal demo up` runs it, and the walkthrough still passes unchanged

## Implementation Notes

### Technical Approach

The sample platform already generates its data from a clock. The events come
from the same place: whatever decides a value changed also sends the
notification, which is exactly what a real platform's write path would do.

A `tokio::sync::broadcast` per platform process, one receiver per subscriber,
mirrors what the shell already does for surfaces.

### Dependencies

[[HLIN-T-0042]] for the manifest fields.

### Risk Considerations

The temptation is to emit on a timer because it is easy, which would make every
later measurement meaningless — the whole claim is that events track real
change. Worth a comment in the code saying so.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

The sample platform serves SSE at `api/events`, declares it, and marks exactly
one panel `pushed`.

**Only one, and that is the interesting part.** Every other panel this platform
serves is a pure function of the clock, which is right for a demo and is also
the one thing that cannot demonstrate an event stream: a value derived from
`now()` has no moment at which it changes. So the platform gained its first real
write path — a count a background task advances — and that panel is the one it
can honestly report on.

It is also deliberately a panel that changes *rarely*. Push saves nothing for
data that genuinely moves eight times a second: the shell would fetch just as
often and would be right to. The case the feature exists for is data that must
be seen promptly and changes seldom, where polling makes you choose between
being late and being wasteful. So `batches` declares `refresh_ms = 250` *and*
offers to be pushed: four requests a second to make a count that moves every few
seconds feel immediate, against a relaxed poll plus a notice. The two `live-*`
panels, whose data really does move eight times a second, do not claim to be
pushed — because they would gain nothing and it would be a lie.

**The order of operations is the platform's whole discipline**, and there is a
test for it: change the data, *then* announce. Announcing first races a shell
fast enough to refetch before the write lands, which produces a notification
that makes the shell read the old value and then not look again until the
relaxed interval — worse than not having sent it.

**A test caught a bug in the change schedule.** The first version multiplied the
tick by an odd constant and took it modulo sixteen, which fires on exactly every
sixteenth tick: multiplying by an odd number modulo a power of two leaves the low
bits' period untouched, so what read as a hash was a metronome — and a metronome
is precisely what could have been polled against instead. The test asserted the
gaps were not all equal and failed. Now splitmix64's finaliser, which exists to
move high bits into low ones.

The event tests run against a real socket rather than `oneshot`, because what is
under test is a response that never ends and an in-process call would return the
first chunk and prove nothing. Five of them: the manifest declares what it
should, a change reaches a subscriber, the data is already changed when the
event arrives, four subscribers all hear it, and a platform with nothing to say
still emits a heartbeat.

333 Rust tests, up from 324. Walkthrough holds, 24 browser tests pass.
