---
id: widgets-four-to-ten
level: task
title: "Widgets four to ten"
short_code: "HLIN-T-0081"
created_at: 2026-09-25T02:43:00.802485+00:00
updated_at: 2026-09-25T12:01:31.001209+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# Widgets four to ten

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 2 of [[HLIN-I-0012]]: `notes`, `dice`, `stopwatch`, `quote`,
`sparkline`, `kanban`, `status`, as described in the initiative's table.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] Seven widget crates, each following the rules below and the pattern
      [[HLIN-T-0080]] set
- [x] `sparkline` declares `time_range` and follows the shell's time picker
- [x] Each added to the twenty flavour's list and to "Twenty"
- [x] Server tests for each widget's rules
- [x] The twenty flavour's browser test still passes, with the new widgets
      `ready`
- [x] `angreal check all`, `angreal test all` pass

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

- Depends on [[HLIN-T-0080]].

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.

### 2026-09-25 (implemented)

**What.** Seven crates under `crates/widgets/`, each copied from the counter
with its own rules as plain functions on its state, a module drawn only with
`--hlin-*` tokens, and `tests/rules.rs` (7 to 11 tests each, 60 in all):

| # | Widget | Port | Shared | Fallback | Rules |
|---|---|---|---|---|---|
| 4 | notes | 8204 | yes | `table` | an edit names its revision; one overtaken by another save is a 409 and the module keeps the draft beside theirs; 500 characters; an unchanged edit moves nothing |
| 5 | dice | 8205 | yes | `table` | the platform rolls (SplitMix64 seeded from `getrandom`, unbiased); d4 to d20, one to six; last ten kept; a retried roll replays, no second chance |
| 6 | stopwatch | 8206 | no | `stat`, 5 s | per `sub`, on the platform so it runs while unmounted; start/stop/lap/reset only when they make sense; twenty laps; the module counts on from arrival with its own clock |
| 7 | quote | 8207 | no | `table`, 5 min | quote of the hour, same for all; skip or pin your own; skip refused while pinned |
| 8 | sparkline | 8208 | no | `timeseries` | declares `time_range`; the module asks for the context's range; at most 60 points on whole-minute steps aligned to the epoch; nothing from the future; 31 days at most; which metric is yours |
| 9 | kanban | 8209 | yes | `table` | anyone adds and moves; only whoever added a card takes it off (403); 80 characters, twelve cards; moves one place, not off either end |
| 10 | status | 8210 | no | `status`, 30 s | made-up five-minute windows per service; since when the run began (a day at most), the last hour, a day's uptime; which service is yours |

All seven are in `WIDGETS`, so "Twenty" carries ten in list order (three
across, `w=4 h=5`); the shell configuration is generated from the list.

**The support crate, additively.** `hlin-widget-support` had no way to
declare a parameter, and its fallback data never saw the range. Rather than a
new `Widget` or `Fallback` field (which would break every widget literal,
including the ones being written in parallel), a fallback whose envelope is
`series.v1` now declares `time_range`, and the fallback route keeps the points
between the `from` and `to` the shell sends. `testing` gains
`fallback_asking`. Every existing manifest is unchanged; one new support test.

**Browser.** `e2e/tests/twenty.spec.js` adds two tests: a kanban card added
in one browser arrives in another (13 to 14 ms) and leaves again when taken
off (79 to 80 ms), which also keeps the column from filling across runs; and
the sparkline follows the picker (1h: a point a minute; 15m: at most 16; 24h:
24-minute steps). No other file needed changing: widgets are placed from the
list.

**Checks.** `angreal check all` clean. `angreal test all` 875 passed, 0
failed, 3 ignored. Against `angreal demo up --with twenty --release`,
`angreal e2e twenty` 4 passed; 10 widgets ready in about 1.6 s.

**Found, not fixed.** Two things for whoever writes the next widgets. In
Leptos's `view!`, an unbraced attribute value containing `>=` (such as
`disabled=move || n >= most`) is cut at the `>` and compiles to something
else, with only an unused-variable warning to say so: brace such values. And
a sandboxed module frame without `allow-forms` never submits a `<form>`, so
the kanban adds on Enter and on the button instead.
