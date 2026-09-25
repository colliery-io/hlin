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

- [x] The cause found and written down: test state or product behaviour
- [x] If product behaviour, fixed, with a test that kills and restarts the
      same platform twice in one session and sees it recover both times
- [x] `twenty-measure` passes three runs in a row on one demo
- [x] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created. Not started.

Reproduced on a fresh `demo up --with twenty --release`: the recovery test
alone passed twice, then the whole of `twenty-measure` failed its recovery
the second time, dice's panel `stale` for the full sixty seconds.

**Product behaviour, not the test's state.** The restarted dice answered the
shell's resubscribe `401 Unauthorized` every thirty seconds, and dice's own
log said why: `refused an identity for this request reason=identity has
expired`. The shell minted the event stream's `hlin-token` headers once, in
`follow_platforms`, when a surface first took an interest, and
`follow_forever` sent those same headers on every reconnect. A token lives
120 s (`TOKEN_LIFETIME_SECONDS`). The first kill came back inside that; on the
second run the subscription (shared, and held since earlier tests' pages) was
older than two minutes, so the platform came back to a shell it would never
again accept, and no `returned` ever told the page to fetch. It would do the
same to any platform restarted more than two minutes after anyone opened its
surface. Backoff was not the cause: it resets after a stream holds 30 s, and
recovery took ~4.8 s once signed properly.

Fix: `streams::Credential`, a function the stream calls for fresh headers on
every attempt; `PlatformView::credentialer` is now an `Arc` so the stream can
hold it. `follow_platforms` still asks once, to skip `forward-session`
platforms. New test `a_platform_restarted_twice_is_subscribed_to_again_both_times`
(tests/events.rs): a platform that closes twice and refuses any credential it
has seen before; it fails with headers minted once and passes now.

Checked: `twenty-measure` passed three full runs in a row on one release demo
(4 passed each, open page back in 4.8–4.9 s), no 401 in the shell log;
`e2e twenty` 6 passed; standard suite with aurora 77 passed; `check all`,
`test all` (1052 passed) pass.

Seen once and not reproduced: in the pre-fix session, `cold and warm` once
found `status` still `loading` after 30 s. Nothing in the shell log about it;
not related to dice, and not seen in the three runs after the fix.
