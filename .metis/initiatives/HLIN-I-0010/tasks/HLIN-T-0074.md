---
id: the-checklist-s-own-module
level: task
title: "The checklist's own module"
short_code: "HLIN-T-0074"
created_at: 2026-09-25T00:39:01.720914+00:00
updated_at: 2026-09-25T01:36:20.848015+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The checklist's own module

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 5 of [[HLIN-I-0010]]. The checklist ships its own UI as a module built
with the SDK ([[HLIN-T-0069]]), hosted by the shell ([[HLIN-I-0011]]).

## Acceptance Criteria

## Acceptance Criteria

- [ ] A Leptos module in the checklist crate (or beside it), built with the
      SDK and the shared kit, served under the platform's `assets` prefix
- [ ] Shows the chosen list's items with a checkbox to cross off, inline edit,
      delete, and an add field; a list picker using `set-param`
- [ ] Refusals shown in the checklist's own words from the platform's answer
- [ ] Announces `changed` after a write, and refetches on `changed` and on
      `context`
- [ ] The manifest's `items` panel gains `ui`; the `table` fallback stays
- [ ] An angreal way to build it that `demo up --with collab` uses

## Implementation Notes

- Blocked on [[HLIN-T-0066]] and [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.
