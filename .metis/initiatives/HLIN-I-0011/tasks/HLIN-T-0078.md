---
id: show-the-time-controls-only-on-a
level: task
title: "Show the time controls only on a surface that has a panel wanting them"
short_code: "HLIN-T-0078"
created_at: 2026-09-25T02:20:28.585895+00:00
updated_at: 2026-09-25T02:20:28.585895+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
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

- [ ] The picker is hidden on a surface where no panel declares `time_range`,
      and shown as soon as one is added in Edit (and hidden again when the
      last is removed)
- [ ] A module panel that declares `time_range` counts, and still receives the
      time range in `context`
- [ ] A surface's stored time range is kept while the picker is hidden, so
      adding a time-driven panel back resumes where it was
- [ ] Browser tests: the collab surface shows no time controls; the standard
      demo surface shows them; adding and removing a time-driven panel shows
      and hides them
- [ ] `angreal check all`, `angreal test all`, `angreal ui build`, and both
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
