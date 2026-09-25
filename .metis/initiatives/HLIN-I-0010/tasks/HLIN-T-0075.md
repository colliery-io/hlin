---
id: the-feed-s-own-module
level: task
title: "The feed's own module"
short_code: "HLIN-T-0075"
created_at: 2026-09-25T00:39:02.766139+00:00
updated_at: 2026-09-25T00:39:02.766139+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The feed's own module

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 6 of [[HLIN-I-0010]]. The feed's UI as a module.

## Acceptance Criteria

- [ ] A Leptos module for the feed, built with the SDK and the shared kit
- [ ] Posts newest first with author and time, a compose box, edit and delete
      on each post; refusals in the feed's own words
- [ ] Announces `changed` after a write, refetches on `changed`
- [ ] The manifest's `posts` panel gains `ui`; the `table` fallback stays
- [ ] Built by the same angreal path as [[HLIN-T-0074]]

## Implementation Notes

- Blocked on [[HLIN-T-0066]] and [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.
