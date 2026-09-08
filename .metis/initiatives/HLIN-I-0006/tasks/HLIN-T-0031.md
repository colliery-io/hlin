---
id: handle-broadcast-lag-without
level: task
title: "Handle broadcast lag without closing the stream"
short_code: "HLIN-T-0031"
created_at: 2026-09-08T01:56:12.595976+00:00
updated_at: 2026-09-08T03:53:04.951944+00:00
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

# Handle broadcast lag without closing the stream

## Objective

`server.rs:200` reads frames with `while let Ok(frame) = receiver.recv().await`.
Tokio's broadcast receiver returns `Err(RecvError::Lagged(n))` when a subscriber
falls more than the buffer behind, then continues from the oldest retained
frame. The loop treats that error like a closed channel and ends the SSE stream.

Finding 6 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P1 - High

### Technical Debt Impact
- **Current Problems**: At 8 Hz with ten panels the 256-frame buffer holds about
  three seconds. A background tab, a slow link, a closed lid: the stream drops,
  panels flash `stale`, `EventSource` reconnects, `current_frames()` resends
  everything. The surface self-heals by accident of two well-designed pieces
  rather than by intent. Under sustained slowness that is a reconnect loop, each
  one a `for_surface` call and a full-state send.
- **Benefits of Fixing**: A slow client skips frames it does not need instead of
  reconnecting.
- **Risk Assessment**: The self-healing hides the symptom and multiplies the cost.

## Acceptance Criteria

- [x] `Lagged(n)` is matched explicitly: logged with the count, then `continue`
- [x] Optionally resend `current()` inline on lag, so the browser is whole
      without a reconnect
- [x] Buffer sized from `panels × per_second` rather than a constant
- [x] Test: a receiver that lags receives later frames on the same stream

## Implementation Notes

### Technical Approach
Frames are idempotent by design — a browser applies the newest generation and
drops the rest — so skipping is correct.

### Dependencies
None.

## Status Updates

### 2026-09-08 — a slow browser skips ahead instead of reconnecting

The SSE handler's `while let Ok(frame) = receiver.recv().await` became a `loop`
with an explicit `match`. `Lagged(n)` is logged with the count and the stream
continues; only `Closed` ends it.

**Skipping is correct rather than merely tolerable.** A browser applies the
newest generation and drops the rest, so the frames a lagging subscriber missed
are ones it would have discarded on arrival. That property is what makes this
safe, and it is why the fix is three lines rather than a replay buffer.

**It also resends `current()` on lag.** Skipped frames may have carried a state
change the browser has now missed entirely — a panel going stale, a platform
withdrawing one — so catching up is not enough; it needs to be made whole. That
is still far cheaper than the reconnect this used to cause, which was a
`for_surface` call and a full-state send anyway, plus a new connection.

**The buffer is sized from the cadence.** A constant 256 was about three
seconds of headroom for ten panels at 8Hz and over an hour at the default half
minute — one number standing for two entirely different things, and on a fast
shell a backgrounded tab could exhaust it in the time it takes to switch
windows. Now `per_second × 30`, clamped to 256–4096: bounded above because this
is memory per surface per viewer, and a browser a full minute behind is better
served by being resent the current state than by a long replay it will mostly
discard.

**Two tests.** One drives a real `broadcast` channel past its buffer and
asserts the receiver reports `Lagged`, then *keeps delivering* — which is the
property the old code threw away. The other pins that a live shell's tick is
finer than a slow one's, so the derived buffer is actually deeper where it
needs to be.

295 Rust tests pass, 0 failures. Browser suite 21 passed, 1 honestly skipped.
`angreal check all` clean. The walkthrough holds.
