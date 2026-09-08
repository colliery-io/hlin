---
id: one-subscription-per-platform-and
level: task
title: "One subscription per platform, and say whether it is healthy"
short_code: "HLIN-T-0048"
created_at: 2026-09-08T12:30:53.909465+00:00
updated_at: 2026-09-08T12:40:18.466236+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# One subscription per platform, and say whether it is healthy

## Parent Initiative

[[HLIN-I-0007]]

## Objective

Two things that turn out to be one thing.

**The open item.** [[HLIN-S-0006]] left stream health on the operator channel
open: an operator should be able to see that a platform's events stopped without
reading logs, because a stream that has quietly died is invisible from every
other angle — the panels still draw, just less currently than the operator
believes.

**The defect found while looking for somewhere to put it.** REQ-2.1 says one
connection per platform. The implementation subscribes inside
`LiveSurface::run`, so it is one connection per platform *per surface*: ten
people watching ten different layouts that each hold an `orebank` panel open ten
event streams to `orebank`. That is precisely the fan-out per-principal
deduplication exists to prevent ([[HLIN-A-0004]]), reintroduced on a new axis.

They are one task because health has to live wherever the connection lives, and
the connection belongs in one place rather than N.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] One subscription per platform for the whole shell, however many surfaces
      hold panels from it
- [ ] It is opened when the first surface needs it and dropped when the last one
      stops, so a shell watching nothing still holds nothing open
- [ ] Every surface holding a pushed panel from that platform is told about its
      events, and told whether it is connected
- [ ] `/api/platforms` carries stream health: whether a stream is declared,
      whether it is connected now, when it last delivered anything, and why the
      last attempt ended
- [ ] A platform that declares no stream reports so, rather than reporting as
      unhealthy — those are different and an operator should not have to guess
- [ ] The five misbehaving platforms still behave as [[HLIN-T-0047]] asserts

## Implementation Notes

### Technical Approach

A registry in the shape of `crate::surfaces::Surfaces`, which already solves the
same problem for surfaces: a map, reference-counted by the handles it hands out,
with the task ending when the last one goes. Broadcast rather than mpsc for the
fan-out, since several surfaces now receive the same event.

Health is whatever that registry already knows, which is the argument for
putting it there rather than inventing a second place to look.

### Dependencies

Everything in this initiative. It is a correction to [[HLIN-T-0044]] and
[[HLIN-T-0046]] rather than new ground.

### Risk Considerations

The subscription lifecycle was the cleanest part of T-0044 — the connection
lived exactly as long as somebody held the receiver, with no bookkeeping. Making
it shared means bringing bookkeeping back, which is where [[HLIN-T-0027]]'s leak
came from. The release path needs the same care: re-check under the lock, and
prove it with a test that starts and stops surfaces.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

Subscriptions moved out of the surface driver into a shared registry, and the
operator endpoint now says what each one is doing.

**The defect was real and the test says so.** `many_surfaces_watching_one_platform_open_one_connection`
counts accepted connections at the platform: ten surfaces, one connection. It
was ten before this.

**Health, on `/api/platforms`, measured against the running demo:**

```
orebank   {"declared":true,"connected":true,"last_heard":"…","endings":0,"watchers":1}
stampmill {"declared":true,"connected":false,"last_heard":null,"endings":0,"watchers":0}
```

`declared` and `connected` are separate on purpose. Three situations would read
identically as one boolean — a platform that offers no stream and is behaving
exactly as intended, one that offers a stream the shell cannot currently hold,
and one whose stream nobody happens to be watching. `stampmill` above is the
third, and an operator should not have to guess which they are looking at.
`endings` and `last_ended` survive reconnection because the case worth catching
is a stream that is up now and keeps falling over, which looks healthy at any
single instant.

**I put the release trigger in the wrong place first, and a test caught it.**
The map holds a strong reference to the refcount, so `Interest::drop` could not
run until the entry had already been removed — which was the thing it was
supposed to cause. Nothing ever closed. The trigger is on `Listening` now, where
dropping actually happens, and `release` re-checks the count under the lock.

That re-check has its own test: a listener taken *between* the last one dropping
and the release running must keep the subscription alive. It is the same race
[[HLIN-T-0027]] documents for surfaces, in a new place, and the ticket predicted
it — making the lifecycle shared meant bringing bookkeeping back, which is where
that leak came from.

371 Rust tests, 24 browser tests, walkthrough holds. HLIN-S-0006 has no open
items left.
