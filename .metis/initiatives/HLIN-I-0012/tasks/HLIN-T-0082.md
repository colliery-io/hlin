---
id: widgets-eleven-to-seventeen
level: task
title: "Widgets eleven to seventeen"
short_code: "HLIN-T-0082"
created_at: 2026-09-25T02:43:01.966175+00:00
updated_at: 2026-09-25T12:00:00.000000+00:00
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

- [x] Seven widget crates, following the rules below and [[HLIN-T-0080]]
- [x] `picker` picks from the people who have used the surface, so it needs
      no directory; `oncall` rotates a fixed roster by week
- [x] Each added to the twenty flavour's list and to "Twenty"
- [x] Server tests for each widget's rules
- [x] The twenty flavour's browser test still passes
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

- Depends on [[HLIN-T-0080]]. Runs alongside [[HLIN-T-0081]]; both append
  to the same widget list, so expect a one-line merge.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.

### 2026-09-25 (implemented)

**What.** Seven crates under `crates/widgets`, each copied from the counter
and made its own: `src/lib.rs` the state and rules as plain functions,
`module/` its Leptos module styled only with `--hlin-*` tokens,
`tests/rules.rs` the rules called as the shell calls them.

| # | Widget | Port | Shared | Rules | Fallback |
|---|---|---|---|---|---|
| 11 | weather | 8211 | no | Weather is a function of city and hour (SplitMix64 noise, seasons the right way round south of the equator, snow only at 1 °C or below), so the same for everybody in an hour; each person's home city and °C/°F | `table`, 10 min refresh |
| 12 | pomodoro | 8212 | no | 25 min focus, 5 min break; one phase at a time; pause keeps what is left; a focus counts once it has run out, when the person moves on; the server keeps when a phase ends and the module counts down on the browser's clock | `stat` (minutes left), 1 min refresh |
| 13 | shoutbox | 8213 | yes | 1 to 280 characters, trimmed; the same words twice running refused; last 50 kept; only the author (by `sub`) takes one back | `table` |
| 14 | reactions | 8214 | yes | Six emoji, one each per person, at most three at once, by `sub` | `table` |
| 15 | bookmarks | 8215 | yes | `http(s)` links with a host, no duplicates (case, trailing slash), title defaults to the host, at most 20; removed by whoever added it, the seeded ones by anybody | `table` |
| 16 | oncall | 8216 | no | Six-name roster rotating by ISO week from 2024-W01, continuous over 53-week years; any week readable as `YYYY-Www`; someone on the roster is told their next turn. No writes, so nothing to announce | `table` (this week and next four), hourly |
| 17 | picker | 8217 | yes | The team is whoever has opened it, remembered by `sub` with their token's current name; a newcomer is announced like a write; each person sits themselves out or back in; a pick is random among those in the draw, never the last one picked while anyone else is in; none in the draw, no pick | `table` (last picks) |

Each is one entry in `WIDGETS` (after the poll, so "Twenty" shows them in
the initiative's order once 4 to 10 land). `e2e/tests/twenty.spec.js` gains
"a shout reaches a second browser without a reload".

**Decisions.**

- The picker remembers people on a read, which `Platform::read` only lets
  look. Rather than change the support crate, the picker keeps its people
  behind a lock of its own inside its state (always taken under the
  platform's, so never contended) and calls `Platform::announce` when a read
  sees someone new. No change to either shared crate.
- Rules that depend on time (pomodoro, weather, oncall) take `now` as an
  argument, so the tests choose the moment; the handlers pass `Utc::now()`.
- The picker's dice are `getrandom::u64()`, passed into the rule as a
  number, so the tests roll their own.
- Shoutbox and bookmarks clear their input after a write only if it still
  says what was sent, so typing during a slow write is not lost.

**Checks.** `angreal check all` clean. `angreal test all` 889 passed, 0
failed, 3 ignored (78 new server tests across the seven, plus two module
unit tests). Against `angreal demo up --with twenty --release` (ten widgets
on this branch), `angreal e2e twenty` 3 passed: 10 widgets ready in
1.6 to 2.3 s; a shout reached the second browser in 13 to 20 ms.

**Found, not fixed:** nothing in the shell, `hlin-ui` or the shared crates.
