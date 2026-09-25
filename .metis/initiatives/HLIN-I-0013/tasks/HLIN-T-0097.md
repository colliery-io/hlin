---
id: widgets-thirteen-to-twenty-on
level: task
title: "Widgets thirteen to twenty on shared components"
short_code: "HLIN-T-0097"
created_at: 2026-09-25T23:56:04.577471+00:00
updated_at: 2026-09-25T23:56:04.577471+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0013
---

# Widgets thirteen to twenty on shared components

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Convert shoutbox, reactions, bookmarks, oncall, picker, deploys, converter, meetings to the shared-components pattern.

Follow exactly the pattern [[HLIN-T-0093]] set and documented in the
widget support crates: a components crate, a core-UI SPA at `/` mounting them
with the direct client, and a module crate that only re-exports them and mounts
them with the Hlin client; `/api/` and `/hlin/api/` over the same handlers;
Hlin under `/hlin`.

## Acceptance Criteria

- [ ] Each widget converted; its module crate contains no UI of its own
- [ ] Each widget's core UI works at `/` on its own (a browser check against
      the widget's port, direct client), and its module works on "Twenty"
- [ ] Server tests still pass; routes under both `/api/` and `/hlin/api/`
      tested where the widget writes
- [ ] `angreal e2e twenty` and `twenty-measure` pass
- [ ] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created. Waits on [[HLIN-T-0093]].
