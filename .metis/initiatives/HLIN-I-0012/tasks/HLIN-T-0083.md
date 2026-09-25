---
id: widgets-eighteen-to-twenty
level: task
title: "Widgets eighteen to twenty"
short_code: "HLIN-T-0083"
created_at: 2026-09-25T02:43:03.009423+00:00
updated_at: 2026-09-25T02:43:03.009423+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# Widgets eighteen to twenty

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 4 of [[HLIN-I-0012]]: `deploys`, `converter`, `meetings`.

## Acceptance Criteria

- [ ] Three widget crates, following the rules below and [[HLIN-T-0080]]
- [ ] `deploys` streams its log to its module over the bridge
      ([[HLIN-T-0068]]), the first real use of streaming
- [ ] `converter` works entirely inside its module and makes no requests,
      and declares no fallback
- [ ] Each added to the list; "Twenty" now has twenty
- [ ] Server tests; the twenty flavour's browser test passes with all twenty
- [ ] `angreal check all`, `angreal test all` pass

Every widget crate:
- lives at `crates/widgets/<name>/` with `src/` (the server) and `module/`
  (its Leptos module on `hlin-module`), both small and readable;
- uses `hlin-widget-support` for scaffolding and keeps its own rules and
  state in its own code;
- declares `ui`, `assets`, `routes` and `events`, and a shell-drawn fallback
  where a natural envelope exists (a counter as `stat`, a poll as `table`);
  a widget with nothing sensible to fall back to (the converter) declares
  none, and says so;
- is themed only through the `--hlin-*` tokens ([[HLIN-S-0007]]);
- announces `changed` after a write and publishes on its event stream, so a
  shared widget updates in every browser;
- has server tests for its rules, as the checklist and feed do.

## Implementation Notes

- Depends on [[HLIN-T-0080]] and [[HLIN-T-0068]].

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.
