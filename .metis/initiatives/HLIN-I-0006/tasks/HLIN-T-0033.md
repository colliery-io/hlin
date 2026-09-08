---
id: one-staleness-clock-for-shell-and
level: task
title: "One staleness clock for shell and browser"
short_code: "HLIN-T-0033"
created_at: 2026-09-08T01:56:14.855809+00:00
updated_at: 2026-09-08T03:47:45.752952+00:00
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

# One staleness clock for shell and browser

## Objective

The shell marks a panel stale after `staleness_ms`, which is configurable. The
browser calls the shell unreachable after `stream_loss_grace_seconds`, which
`/api/config` serves as the literal `30` (`server.rs:74`). With
`demo/hlin-live.toml` the shell's staleness is 2 s and the browser's is 30 s.

Finding 8 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P2 - Medium

### Technical Debt Impact
- **Current Problems**: A person sees data go stale in two seconds when a
  platform is slow and in thirty when the shell is gone, and nothing on screen
  explains the difference.
- **Benefits of Fixing**: One clock, set in one place.
- **Risk Assessment**: Low, but it makes the degradation story incoherent to
  anyone who tunes the timings.

## Acceptance Criteria

- [x] `client_config` serves a grace derived from `Timings` — `staleness_ms`, or
      a dedicated field beside it
- [x] The browser test for stream loss reads its expectation from `/api/config`
      rather than a constant

## Implementation Notes

### Technical Approach
`server.rs:70` `client_config` already has `state.config`; the literal is the
whole of the bug.

### Dependencies
None.

## Status Updates

### 2026-09-08 — one clock, bounded at both ends

`Timings::stream_loss_grace()` derives the browser's grace from `staleness_ms`,
and `client_config` serves it instead of the literal `30`.

**Not simply equal to staleness**, which is where the first attempt went wrong.
I made them identical, and the degradation test then raced a ninety-second
grace with a ninety-second timeout and could never win. That failure was worth
having: it made the design question concrete. Staleness is about a platform's
data aging, and ninety seconds of that is right for a panel refreshed every
thirty. Stream loss is about the shell being gone, and ninety seconds of a
person staring at a dashboard before anything says so is not a grace, it is a
hang.

So it is clamped to 5–30 seconds. The floor is a reconnect: an `EventSource`
recovers on its own within a few seconds, and anything shorter would report the
shell as gone during an ordinary one. The ceiling is a person's patience.

```
staleness 90s (default)  -> grace 30s   (capped; what the literal used to be)
staleness 20s            -> grace 20s   (follows)
staleness  2s (live)     -> grace  5s   (floored)
```

`/api/config` also now reports `refresh_ms`. That is not decoration: a suite
asserting a panel redraws several times a second is asserting something only
some configurations can deliver, and it had no way to know which shell it was
talking to. `live.spec.js` now skips honestly on a slow shell instead of
failing — which was one of the failures in every run of [[HLIN-T-0041]].

The degradation test reads its timeout from `/api/config` rather than hardcoding
one, per the second criterion. Two config unit tests pin the derivation and its
bounds.

`angreal check all` clean. Rust suite passing.
