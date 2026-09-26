---
id: widgets-four-to-twelve-on-shared
level: task
title: "Widgets four to twelve on shared components"
short_code: "HLIN-T-0096"
created_at: 2026-09-25T23:56:04.549676+00:00
updated_at: 2026-09-26T00:50:46.812491+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: true
initiative_id: HLIN-I-0013
---

# Widgets four to twelve on shared components

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Convert notes, dice, stopwatch, quote, sparkline, kanban, status, weather, pomodoro to the shared-components pattern.

Follow exactly the pattern [[HLIN-T-0093]] set and documented in the
widget support crates: a components crate, a core-UI SPA at `/` mounting them
with the direct client, and a module crate that only re-exports them and mounts
them with the Hlin client; `/api/` and `/hlin/api/` over the same handlers;
Hlin under `/hlin`.

## Acceptance Criteria

## Acceptance Criteria

- [x] Each widget converted; its module crate contains no UI of its own
- [x] Each widget's core UI works at `/` on its own (a browser check against
      the widget's port, direct client), and its module works on "Twenty"
- [x] Server tests still pass; routes under both `/api/` and `/hlin/api/`
      tested where the widget writes
- [x] `angreal e2e twenty` and `twenty-measure` pass
- [x] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created. Waits on [[HLIN-T-0093]].

### 2026-09-26 — notes to pomodoro converted

- **Per widget**, as `hlin-widget-support`'s *Converting a widget* says:
  `components/` (the old module's `main.rs` as `lib.rs`, its `module.css` as
  `style.css`, the `start` closure as `#[component] pub fn <Name>`), `ui/`
  (`direct::mount(<Name>)`), a `module/` that is `pub use` of the components
  and `hlin_widget_module::mount`, and the server's `run_with(…, Builds)` with
  an `embed` feature. No trait change was needed: `widget.module().visible()`
  is `widget.visible()`, the sparkline's `context().time_range` is
  `widget.time_range()`, the note's draft uses `widget.restored()` and
  `widget.on_suspend(…)`. Refusal words, *Try again* and the changed
  announcements come from the shared `Widget` handle.
- **Tests.** Each widget's `tests/rules.rs` gained one test writing through
  its own `/api/` as the local user (`start_site` with a `Site`), read back
  through `/hlin/api/` where the widget is shared (notes, dice, kanban) or its
  own `/api/` where it is per person; the existing tests cover `/hlin/api/`.
- **e2e.** The nine are in `OWN_UI`; a new test in `twenty.spec.js` draws each
  one's own UI at its port with no refusal or failure, and rolls the dice on
  its own page and sees the roll reach its module on "Twenty".
- **Verified.** `angreal check all`; `angreal test all` 1078 passed; after
  `demo up --with twenty --release`, `e2e twenty` 8 passed (one earlier run
  timed out on the shoutbox, not yet converted, finding its textbox; passed
  on the rerun) and `e2e twenty-measure` 4 passed, including the note's
  draft surviving being scrolled away and back.
- `legacy.rs` stays for [[HLIN-T-0097]]'s eight.
