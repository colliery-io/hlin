---
id: show-the-time-controls-only-on-a
level: task
title: "Show the time controls only on a surface that has a panel wanting them"
short_code: "HLIN-T-0078"
created_at: 2026-09-25T02:20:28.585895+00:00
updated_at: 2026-09-25T02:43:43.701035+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: true
initiative_id: HLIN-I-0011
---

# Show the time controls only on a surface that has a panel wanting them

## Parent Initiative

[[HLIN-I-0011]]

## Objective

The owner, 2026-09-24: "The shell should [not] have the time widgets unless
someone wants them." Today the time picker (15m, 1h, 6h, 24h, the two
date-time fields and Apply) is drawn on every surface. On the collaborative
demo's surface nothing uses time: a to-do list and a feed. Controls that do
nothing are noise, and in a workspace they are most of the bar.

Rule: the time controls appear only when at least one panel on the surface
declares the `time_range` parameter, whether the shell draws it or a module
does. Otherwise they are absent, not disabled.

## Acceptance Criteria

## Acceptance Criteria

- [x] The picker is hidden on a surface where no panel declares `time_range`,
      and shown as soon as one is added in Edit (and hidden again when the
      last is removed)
- [x] A module panel that declares `time_range` counts, and still receives the
      time range in `context`
- [x] A surface's stored time range is kept while the picker is hidden, so
      adding a time-driven panel back resumes where it was
- [x] Browser tests: the collab surface shows no time controls; the standard
      demo surface shows them; adding and removing a time-driven panel shows
      and hides them
- [x] `angreal check all`, `angreal test all`, `angreal ui build`, and both
      e2e flavours pass

## Implementation Notes

- `crates/hlin-ui/src/app.rs` draws the bar. The catalog and `/api/platforms`
  carry each panel's `params`, so the page knows which panels declare
  `time_range` without asking.
- Waits for [[HLIN-T-0070]] to merge, which is editing `app.rs` now.
- If the reading "should not have" is wrong, the owner will say so; this was
  recorded from a message with a likely typo.

## Status Updates

### 2026-09-24

Created from the owner's request. Not started; queued behind HLIN-T-0070.

### 2026-09-24 (implemented)

Built on `2717acb` (HLIN-T-0070 merged).

- `LayoutDraft::wants_time(catalog)` (`crates/hlin-ui/src/draft.rs`) is the
  decision: true when any panel's catalogue entry lists `time_range` in its
  `params`, whoever draws it. A panel the catalogue does not know counts for
  nothing, so before the catalogue answers the controls are absent and
  appear once it does.
- `app.rs` wraps the picker (presets, date fields, Apply) in a `Show` on a
  memo of that, so it is absent, not disabled. The `applied` indicator
  stays: it also covers per-panel selections.
- Stored range kept: the picker's signals are untouched while hidden, the
  surface is still asked for that range, and modules are still told it as
  `context`; a time-driven panel added back resumes it (browser-tested with
  6h).
- Decision, pages: a page shows no time controls. A navigation entry declares
  nothing about time; the page's module is still told the surface's range as
  `context`. Back on the surface they return.

Checks: `angreal check all` clean; `angreal test all` 728 passed;
`angreal ui build` ok. Browser: standard (`--with aurora`, `angreal e2e test`)
47 passed, 18 skipped; collab (`--with collab`) `angreal e2e signin` 9 passed,
`angreal e2e walkthrough` 3 passed. New: collab bar has no time controls;
empty surface has none; add/remove/re-add a time-driven panel in Edit shows,
hides and restores them with the chosen preset; a page hides them.
