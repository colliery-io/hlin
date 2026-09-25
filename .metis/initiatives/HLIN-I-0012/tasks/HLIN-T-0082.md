---
id: widgets-eleven-to-seventeen
level: task
title: "Widgets eleven to seventeen"
short_code: "HLIN-T-0082"
created_at: 2026-09-25T02:43:01.966175+00:00
updated_at: 2026-09-25T11:20:44.499582+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# Widgets eleven to seventeen

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 3 of [[HLIN-I-0012]]: `weather`, `pomodoro`, `shoutbox`,
`reactions`, `bookmarks`, `oncall`, `picker`.

## Acceptance Criteria

## Acceptance Criteria

- [ ] Seven widget crates, following the rules below and [[HLIN-T-0080]]
- [ ] `picker` picks from the people who have used the surface, so it needs
      no directory; `oncall` rotates a fixed roster by week
- [ ] Each added to the twenty flavour's list and to "Twenty"
- [ ] Server tests for each widget's rules
- [ ] The twenty flavour's browser test still passes
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

- Depends on [[HLIN-T-0080]]. Runs alongside [[HLIN-T-0081]]; both append
  to the same widget list, so expect a one-line merge.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.
