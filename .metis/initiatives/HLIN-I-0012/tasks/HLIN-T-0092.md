---
id: the-twenty-measurement-fails-its
level: task
title: "The twenty measurement fails its recovery test when run twice on one demo"
short_code: "HLIN-T-0092"
created_at: 2026-09-25T22:12:24.994239+00:00
updated_at: 2026-09-25T22:21:46.832771+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# The twenty measurement fails its recovery test when run twice on one demo

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Found by [[HLIN-T-0091]]: `angreal e2e twenty-measure` passes on a fresh
`demo up --with twenty --release`, but a second run on the same demo fails
"killing dice's platform degrades its panel alone, and it recovers by
itself". It fails the same way on `1745814`, before [[HLIN-T-0091]], so it
predates that change. Either the test leaves state behind (dice restarted by
`angreal demo restart` under a different registry entry or port binding, the
shell's event-stream backoff grown to its 30 s ceiling, the page's
three-refusal count, contract memory), or recovery really does work only the
first time. The second would be a product bug.

## Acceptance Criteria

## Acceptance Criteria

- [ ] The cause found and written down: test state or product behaviour
- [ ] If product behaviour, fixed, with a test that kills and restarts the
      same platform twice in one session and sees it recover both times
- [ ] `twenty-measure` passes three runs in a row on one demo
- [ ] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created. Not started.
