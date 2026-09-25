---
id: walk-two-people-through-the
level: task
title: "Walk two people through the collaborative demo, in two browsers"
short_code: "HLIN-T-0077"
created_at: 2026-09-25T00:39:04.954126+00:00
updated_at: 2026-09-25T02:28:55.909802+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
initiative_id: HLIN-I-0010
---

# Walk two people through the collaborative demo, in two browsers

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 8 of [[HLIN-I-0010]]: the initiative's exit criterion, asserted.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] Playwright with two browser contexts signed in through Dex as Alice and
      Bob: Alice adds an item and it appears for Bob without a reload; Bob
      crosses it off and Alice sees it crossed
- [x] Bob edits Alice's post and sees the feed's refusal in its own words
- [x] Carol reads the feed, is refused when she posts, and is refused the team
      list
- [x] Screenshots per step, as the existing suite does
- [x] Runnable by an angreal task against `demo up --with collab`

## Implementation Notes

- Depends on everything else in [[HLIN-I-0010]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 — note

Newcomers land on a surface of their own ([[HLIN-T-0076]], decided). Bob and
Carol reach "The team" by its link in the walkthrough, not by signing in.

### 2026-09-24 — implemented

**What.** `e2e/tests/walkthrough.spec.js`, run by a new `angreal e2e
walkthrough` (which, like `e2e signin`, refuses clearly when nothing answers
or the shell does not sign people in; the shared checks are now
`_collab_ready()` in `task_e2e.py`). Three serial tests:

1. Alice and Bob, two browser contexts signed in through Dex at once, both on
   "The team" (Bob by its link, which is where Alice lands). Alice adds an
   item; Bob's open page shows it, authored by Alice, uncrossed. Bob crosses
   it off; Alice's open page shows it crossed and checked.
2. Alice posts; Bob's open feed shows it; Bob's edit is refused with "Only the
   author can edit this post"; cancelled, the post is as Alice wrote it.
3. Carol, in a third context: reads Alice's post and the seeded one; her post
   is refused with "Only people at example.com can post here" and not listed;
   the checklist refuses her the team list in its words and shows none of it.

"Without a reload" is asserted, not assumed: each page counts its `load`
events after opening and the test fails if any came. No fixed sleeps: each
cross-browser step waits up to 15 s for the other page, and prints how long it
took. Screenshots 110–120 (`walkthrough-*`).

**Found.** Propagation already worked end to end (platform event stream →
shell → surface stream `changed` frame → bridge `changed` → module refetch);
nothing needed fixing. Measured over two runs: Alice's item reaches Bob in
28–50 ms, Bob's tick reaches Alice in 81–86 ms, Alice's post reaches Bob in
13–42 ms (from the moment the author's own page showed it).

Also: README's "Not yet" about cross-browser changes replaced with how it
works and the new command; `demo up --with collab` mentions it;
collab-modules.spec.js's pointer to this task now points at the spec.

**Checks.** `angreal e2e walkthrough` 3 passed (twice); `angreal e2e signin`
9 passed; `angreal check all` clean; `angreal test all` 716 passed, 0 failed.
