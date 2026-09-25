---
id: count-a-frame-against-the-budget
level: task
title: "Count a frame against the budget until it has left the page"
short_code: "HLIN-T-0085"
created_at: 2026-09-25T12:37:18.677215+00:00
updated_at: 2026-09-25T17:09:22.014123+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Count a frame against the budget until it has left the page

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0084]]: scrolling "Twenty" peaks at 18 mounted frames,
against a budget of 12 ([[HLIN-S-0007]] *Budget*, REQ-4.3). The page stops
counting a frame the moment it sends `suspend`, but the frame stays in the
document for up to the 500 ms `suspend` deadline, and more when a module never
answers: the SDK does not answer `suspend` at all when a module registered no
hook, so every such frame waits out the whole deadline.

## Acceptance Criteria

## Acceptance Criteria

- [x] A frame counts against the budget from mount until it is removed from
      the document, including while it is being suspended
- [x] The SDK answers `suspend` at once with an empty `state` when the module
      registered no hook, so a module that keeps nothing costs no wait
- [x] The twenty measurement's peak mounted-frame count is at most 12 while
      scrolling top to bottom and back; the test asserts the peak, not the
      settled count
- [x] Still true: no frame in view is ever unmounted
- [x] `angreal check all`, `angreal test all`, `angreal e2e twenty` and the
      standard suite pass

## Implementation Notes

- `crates/hlin-ui/src/frame.rs` (budget, `detach`), `crates/hlin-module`
  (suspend handling), `e2e/tests/twenty-measure.spec.js`.

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.

### 2026-09-25 — done

- **Budget.** `bridge::over_budget` counts every frame in the document; one
  already being suspended (`Mounted::leaving`) is counted but not chosen
  again. New `bridge::room_for`: at the budget, a frame is not mounted until
  room is made. `frame::attach` marks the host `waiting` and lets the chosen
  frame go (`let_go`: `suspend`, or at once if it cannot hear); when a frame
  leaves the document (`detach`, `unmount`) `mount_waiting` mounts whatever
  waited, those in view first. A frame in view is mounted past twelve only
  when nothing out of view can go. A page (navigation entry) is never
  counted and never waits.
- **SDK.** `hlin-module` answers `suspend` at once with `state { blob: null }`
  when there is no hook or it keeps nothing.
- **Spec.** HLIN-S-0007 *Budget* and `state` say both.
- **Measured** (`angreal e2e twenty-measure`, release, three runs): peak
  **12** in every run (was 18), settled 12; the test now asserts the peak.
  Nothing in view was unmounted. Cold ready 577 ms, warm 532 ms (unchanged).
- `angreal check all`, `angreal test all` (1021 passed), `angreal ui build`,
  standard suite `--with aurora` (76 passed, 28 skipped), `angreal e2e
  signin` (9), `angreal e2e twenty` (6; one run of three failed once on "a
  shout reaches a second browser", passing on both reruns) and
  `twenty-measure` (4) pass.
