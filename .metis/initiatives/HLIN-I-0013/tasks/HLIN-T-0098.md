---
id: find-why-the-twenty-suites-time
level: task
title: "Find why the twenty suites time out once in a while"
short_code: "HLIN-T-0098"
created_at: 2026-09-26T01:17:50.539837+00:00
updated_at: 2026-09-26T01:17:50.539837+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
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

- [ ] The twenty suites run in a loop (say twenty runs) with traces and the
      shell's and widgets' logs kept for each failure
- [ ] Each failure's cause found and written down: test timing, or product
- [ ] Product causes fixed with a test; test causes fixed without lengthening
      timeouts to hide them
- [ ] Twenty consecutive runs of each suite pass

## Status Updates

### 2026-09-25

Created after the second intermittent failure. Not started; best done after
[[HLIN-T-0097]] lands so all twenty are on the new pattern.
