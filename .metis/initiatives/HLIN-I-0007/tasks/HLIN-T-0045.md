---
id: an-event-moves-a-fetch-earlier
level: task
title: "An event moves a fetch earlier, coalesced and jittered"
short_code: "HLIN-T-0045"
created_at: 2026-09-08T11:03:19.067845+00:00
updated_at: 2026-09-08T11:50:16.004728+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# An event moves a fetch earlier, coalesced and jittered

## Parent Initiative

[[HLIN-I-0007]]

## Objective

Turn an event into a fetch that happens sooner, without turning one event into a
thundering herd.

An event marks matching panel instances due. Matching is by subset: every key
the event names must match on the instance, and keys it does not name are not
consulted (HLIN-S-0006).

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] An event marks matching instances due on every live surface
- [ ] An event for a panel nobody is watching causes no fetch at all (REQ-2.2)
- [ ] Subset matching, with tests for the empty case, a narrowing case, and an
      instance that differs on a key the event did not mention
- [ ] Event-prompted fetches are coalesced within a window and jittered across
      it, so one event for a widely watched panel does not fire N simultaneous
      requests (REQ-2.3)
- [ ] Repeated events for one instance inside the window collapse to one fetch
- [ ] The refresh floor applies unchanged: a platform emitting a thousand events
      a second is clamped exactly as a panel asking for `refresh_ms = 1` is
      (REQ-2.4)
- [ ] Every row of the outcome table in [[HLIN-S-0003]] still holds, because the
      fetch is the same fetch

## Implementation Notes

### Technical Approach

In `stream::aggregator`, which already owns "what is due" and is tested against
a clock the test controls — so the coalescing window and the jitter are testable
without sockets, which is the reason that separation exists.

### Dependencies

[[HLIN-T-0044]] for events to arrive.

### Risk Considerations

Jitter makes tests non-deterministic if done carelessly. The window and the
jitter source both belong in the policy the test constructs, not in the code
that uses them.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

`Surface::changed()` marks matching instances due sooner. Nothing is fetched
there: it moves a moment, and the tick that was always going to run does the
rest — which is what keeps push from being a second code path with its own
failure modes. Every row of the outcome table in [[HLIN-S-0003]] still describes
the fetch, because it is the same fetch.

**Coalescing falls out of the representation.** The nudge is a moment rather
than a flag, so a second event arriving before the first has been acted on finds
it already set and changes nothing. Fifty notices, one fetch, with no counter
and no separate window bookkeeping.

**The jitter is derived from the instance id, not drawn at random.** Twenty
instances land at twenty different points in the window, the same instance lands
in the same place every time, and the test can assert on it. A random offset
would have made every test here either flaky or forced to inject a generator,
and would have bought nothing: the property wanted is that distinct instances
differ, not that nobody can guess them.

**The floor applies to event-prompted fetches.** A platform emitting a thousand
events a second gets exactly what a panel asking for `refresh_ms = 1` gets. The
test tells a surface constantly for four seconds against a five-second floor and
asserts zero fetches, then one after it elapses.

Seven cases, all against a clock the test controls: earlier than the cadence,
nothing for a panel nobody watches, fifty events into one fetch, the floor
holding under a flood, twenty instances not firing together, subset matching by
selections, and a retired panel never fetched however loudly its platform
shouts.

Fixed in passing: the comment on `refresh_floor` in `config.rs` still described
it as "a quarter of the shell's interval", which is what it was before
[[HLIN-T-0036]] made it absolute — actively misleading about the thing that task
existed to fix.

354 Rust tests, up from 333.
