---
id: find-why-the-twenty-suites-time
level: task
title: "Find why the twenty suites time out once in a while"
short_code: "HLIN-T-0098"
created_at: 2026-09-26T01:17:50.539837+00:00
updated_at: 2026-09-26T05:38:23.112751+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
initiative_id: HLIN-I-0013
---

# Find why the twenty suites time out once in a while

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Two intermittent failures, each passing on a re-run:

- [[HLIN-T-0093]]: the first `twenty-measure` run failed once in "cold and
  warm", waiting for a middle-row widget's first content.
- [[HLIN-T-0096]]: the first `e2e twenty` run timed out on shoutbox.

Also once before, in [[HLIN-T-0092]]: `status` stuck `loading` in the cold
and warm test. Each is rare, but a suite that fails one run in a few is a
suite people stop believing, and at twenty frames this may be a real bug
(a lost `init`, a mount the budget never completes, a first fetch that never
answers) rather than a slow machine.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] The twenty suites run in a loop (say twenty runs) with traces and the
      shell's and widgets' logs kept for each failure
- [x] Each failure's cause found and written down: test timing, or product
- [x] Product causes fixed with a test; test causes fixed without lengthening
      timeouts to hide them
- [x] Twenty consecutive runs of each suite pass

## Status Updates

### 2026-09-25

Created after the second intermittent failure. Not started; best done after
[[HLIN-T-0097]] lands so all twenty are on the new pattern.

### 2026-09-26 — one cause, already fixed; now tested

- **Method.** Each suite looped with `npx playwright test <spec>` (traces
  `retain-on-failure`), each failure's `e2e/results` and the part of every
  `demo/state/logs/*.log` written during that run kept apart, on
  `demo up --with twenty --release` at 0b5670c.
- **After [[HLIN-T-0097]]'s fix: no failure in 104 runs.** `e2e twenty` 20 of
  20 and `twenty-measure` 20 of 20 on one demo (load 2 to 12 while
  [[HLIN-T-0094]] built its images); 4 cycles of a fresh `demo up` and the
  first run of each on it, since all three failures seen before were first
  runs (8 of 8); 8 more such cycles with eight busy loops on the twelve cores
  (16 of 16); and the final 20 and 20 below.
- **With the fix reverted: it comes back, in both suites, in the shapes seen
  before.** `e2e twenty` 1 of 8 failed as [[HLIN-T-0096]]'s did: "a shout
  reaches a second browser", the second browser's shoutbox, reactions and
  bookmarks in view with no frame. `twenty-measure` 1 of 8 failed as
  [[HLIN-T-0093]]'s and [[HLIN-T-0092]]'s did: cold and warm, the middle
  screen, `status` `loading` for 30 s with no frame (its heading and nothing
  else in the snapshot). So all three were this bug: product, not timing.
- **Why so often.** A logged build showed it: the browser reports the page's
  intersection observers in no fixed order (not panel order), so in one
  scroll the page could pick, to make room for a panel coming into view, a
  frame it had not yet been told was coming into view too. That frame then
  stayed, and before the fix nothing asked for room again; the module's
  answer to `suspend` was ignored because the suspension had been cancelled.
- **The test the fix lacked** (`bridge.spec.js`, *the frame budget, a frame
  that stays*, standard suite): every module ignores `suspend`; scrolled
  until a tall panel is just out of view, twelve are mounted; the window made
  taller brings the last panel into view, and the tall one, the only frame
  out of view, is asked to go; scrolled back a little, its edge is in view
  again. One step at a time, so observer order cannot matter. Without the
  fix it fails 6 of 6 (the last panel never gets a frame); with it 10 of 10,
  and 10 of 10 with every core busy.
- **No test cause found**, so no test timing was changed.
- **Final:** `e2e twenty` 20 consecutive passes (55–58 s each),
  `twenty-measure` 20 consecutive passes (122–124 s each). Standard suite
  (`--with aurora`) 78 passed; `angreal check all`; `angreal test all` 1085
  passed.
