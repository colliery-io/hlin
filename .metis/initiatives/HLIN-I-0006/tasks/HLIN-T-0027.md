---
id: stop-a-surface-when-its-last
level: task
title: "Stop a surface when its last viewer leaves"
short_code: "HLIN-T-0027"
created_at: 2026-09-08T01:56:07.337912+00:00
updated_at: 2026-09-08T02:53:18.233089+00:00
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

# Stop a surface when its last viewer leaves

## Objective

A `LiveSurface` runs until the process exits. `Surfaces::live` only shrinks in
`forget()`, called on a layout write or delete; `LiveSurface::run` is `loop {}`
with no exit; nothing reads `receiver_count()`. Every surface any browser, test
or `curl` has ever subscribed to is still polling its platforms.

Finding 2 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P0 - Critical: unbounded upstream load, growing with every viewer ever

### Technical Debt Impact
- **Current Problems**: Measured on the demo with zero viewers connected:

  ```
  stampmill non-manifest log lines: 45447 -> 45866 over 5s idle (delta 419)
  orebank   non-manifest log lines: 106184 -> 106938 over 5s idle (delta 754)
  ```

  84 and 151 requests per second from surfaces nobody is watching. At the
  30-second default this is a slow leak; at `refresh_ms = 125` it is a load test
  against every platform.
- **Benefits of Fixing**: Upstream load becomes a function of who is looking.
- **Risk Assessment**: A long-running shell degrades every platform it fronts,
  in proportion to its own popularity.

## Acceptance Criteria

- [x] The driver stops and the surface leaves the map after `receiver_count()`
      has been zero for an idle grace (seconds, so a reconnecting browser does
      not rebuild from scratch)
- [x] Removal happens under the same lock `for_surface` takes, so a subscriber
      arriving during teardown gets a fresh surface, never a dying one
- [x] Test: subscribe, drop the receiver, advance time, assert the platform is
      no longer asked
- [x] The measurement above, repeated, reads zero

## Implementation Notes

### Technical Approach
Where: `crates/hlin/src/surfaces.rs:108`, `crates/hlin/src/stream/live.rs:91`.
`broadcast::Sender::receiver_count()` already knows. Check it in the existing
once-a-second branch; on expiry, remove self from `Surfaces` and return from
`run`. Holding the map entry as `Weak<LiveSurface>` is the alternative.

### Dependencies
Interacts with [[HLIN-T-0035]]: a surface dropped on layout write must also stop.

### Risk Considerations
`for_surface` and the driver must agree on one lock, or a race recreates the
orphaned-surface bug fixed in [[HLIN-I-0002]].

## Status Updates

### 2026-09-08 — 230 requests a second, down to zero

`LiveSurface::run` now takes a `Weak<Surfaces>` and its own key, and takes
itself out of the map when nobody has been watching for `IDLE_GRACE` (30s).
`Surfaces::release` makes the decision under the same lock `for_surface` takes,
and `for_surface` became `self: &Arc<Self>` so the driver can be handed a weak
reference home. `Weak` rather than `Arc` because the map holds the surface, and
an `Arc` both ways is a cycle that frees neither.

**Measured, on the running demo, which is the acceptance criterion:**

```
before:  orebank 151/s, stampmill 84/s   (nobody watching)
after:   orebank   0/s, stampmill   0/s  (nobody watching, grace elapsed)
```

And the grace does its job in the other direction: five seconds after a
disconnect, still inside it, orebank was still being asked (19 requests in 3s),
and the shell logged one `stopped a surface nobody was watching` after thirty.

**Three cases the release path has to get right**, all in the test:

- A surface with a subscriber is never released, or the browser attached to it
  is stranded exactly as the orphaned-surface bug stranded one.
- A released surface leaves the map, so the next subscriber builds a fresh one
  rather than attaching to a driver that has stopped.
- An old driver releasing must not remove the surface that replaced it under
  the same key. `Arc::ptr_eq` is what makes that safe, and a layout write makes
  it happen routinely.

A surface also starts with `unwatched_since` set rather than clear, so one built
by a parameter POST that is never subscribed to stops on its own. That is the
same pair of requests that caused the race `for_surface` already guards.

282 Rust tests pass, 0 failures. `angreal check all` clean. The walkthrough
holds.

### The browser suite, honestly

21 of 22 pass. `surface.spec.js` "removing a panel takes it off the grid for
good" fails in a *full* run and passes when its file runs alone — it finds zero
panels where two should be.

Not caused by this change: I stashed the change, rebuilt, and got the identical
failure. It is cross-file load sensitivity in the suite, which is now
consistently reproducible rather than the one-in-three flake recorded in
[[HLIN-I-0004]]. Filed as [[HLIN-T-0041]] rather than fixed here, because it is
a different problem and this task's own evidence is the measurement above.
