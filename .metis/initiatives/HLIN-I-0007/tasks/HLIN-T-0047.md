---
id: prove-the-shell-survives-five
level: task
title: "Prove the shell survives five badly behaved event streams"
short_code: "HLIN-T-0047"
created_at: 2026-09-08T11:03:19.109392+00:00
updated_at: 2026-09-08T12:20:24.944373+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# Prove the shell survives five badly behaved event streams

## Parent Initiative

[[HLIN-I-0007]]

## Objective

Prove the claim the specification actually makes: that no behaviour of a
platform's event stream can make the shell worse than it was when it only
polled.

Five platforms on a bad day, as fixtures.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] A platform that declares a stream and never sends anything: panels stay
      current within the relaxed interval
- [ ] A platform whose events name panels that do not exist: discarded, no
      fetches, stream stays open
- [ ] A platform that closes the stream mid-session: reconnects, and panels are
      on `refresh_ms` while it is gone
- [ ] A platform that holds the connection open and goes quiet without closing
      it: detected by the missed heartbeat — **write this one first**, it is the
      only failure with no signal of its own
- [ ] A platform that floods events: coalesced, jittered, clamped by the floor,
      and the shell's upstream rate stays bounded
- [ ] For all five: the browser sees what it would have seen from polling alone

## Implementation Notes

### Technical Approach

Fixtures in the shell's own test suite rather than variants of the sample
platform: these are servers that exist to misbehave, and a real platform should
not carry the code to do it.

The last one needs an actual held socket that stops writing. Nothing mocked will
exercise the read timeout that is the whole point.

### Dependencies

Everything above.

### Risk Considerations

A suite that only tests the happy path would let all five of these ship broken,
and four of them fail silently in production. This is the task that makes the
feature safe to adopt, not the one that makes it work.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

Eight cases in `crates/hlin/tests/misbehaving.rs`, running in 0.46 seconds
against real sockets. Six platforms on a bad day, plus two that assert the thing
the specification actually claims: that a viewer ends up where polling alone
would have left them.

The quiet-socket case was written first, as the ticket said. Four of the others
fail loudly; that one is the only failure with no signal of its own, and a suite
that skipped it would have let the silent failure ship.

**The flood case found a design defect rather than confirming a design.** The
test deadlocked: two thousand events against a bounded channel, and `send().await`
blocked the reader waiting for a consumer that was not draining yet. In
production the driver drains every tick so it would not have deadlocked — but it
would have done something worse and quieter. Blocking the reader stops reading
the socket, which pushes back on the *platform*, making the shell's own
consumption rate a platform's problem. It is not one.

So a full channel now drops. That is safe for exactly the reason the whole
feature is safe: an event is news the shell is free to miss, because the poll
underneath means the worst case is one relaxed interval of staleness — strictly
better than holding a platform's write path open. Logged once per power of two
rather than per event, because a platform that floods would otherwise flood the
log as well.

**A sixth case I had not planned**: a platform answering an event request with a
login page, which is what a shell behind an expired credential actually gets.
Read as a stream it is an endless supply of nothing, and without the content-type
check the shell would believe it was subscribed forever while every panel of that
platform quietly went stale. That is the same silent-failure shape as the quiet
socket, reached by a different route.

367 Rust tests, 24 browser tests, walkthrough holds.
