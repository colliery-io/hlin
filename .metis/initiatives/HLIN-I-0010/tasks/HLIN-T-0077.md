---
id: walk-two-people-through-the
level: task
title: "Walk two people through the collaborative demo, in two browsers"
short_code: "HLIN-T-0077"
created_at: 2026-09-25T00:39:04.954126+00:00
updated_at: 2026-09-25T00:39:04.954126+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# Walk two people through the collaborative demo, in two browsers

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 8 of [[HLIN-I-0010]]: the initiative's exit criterion, asserted.

## Acceptance Criteria

- [ ] Playwright with two browser contexts signed in through Dex as Alice and
      Bob: Alice adds an item and it appears for Bob without a reload; Bob
      crosses it off and Alice sees it crossed
- [ ] Bob edits Alice's post and sees the feed's refusal in its own words
- [ ] Carol reads the feed, is refused when she posts, and is refused the team
      list
- [ ] Screenshots per step, as the existing suite does
- [ ] Runnable by an angreal task against `demo up --with collab`

## Implementation Notes

- Depends on everything else in [[HLIN-I-0010]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 — note

Newcomers land on a surface of their own ([[HLIN-T-0076]], decided). Bob and
Carol reach "The team" by its link in the walkthrough, not by signing in.
