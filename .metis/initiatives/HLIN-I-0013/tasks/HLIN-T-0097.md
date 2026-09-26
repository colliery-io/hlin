---
id: widgets-thirteen-to-twenty-on
level: task
title: "Widgets thirteen to twenty on shared components"
short_code: "HLIN-T-0097"
created_at: 2026-09-25T23:56:04.577471+00:00
updated_at: 2026-09-26T00:50:46.838721+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
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

### 2026-09-26 — the eight converted

- **Per widget**, exactly the seven steps in `hlin-widget-support`'s docs:
  `components/` (the old `module/src/main.rs` and `module.css`, moved with
  `git mv`), `ui/` (`direct::mount`), `module/` (`pub use` and
  `hlin_widget_module::mount`, nothing else), and `run_with(…, Builds {…})`
  with an `embed` feature. Components: `Shoutbox`, `Reactions`, `Bookmarks`,
  `Oncall`, `Picker`, `Deploys`, `Converter`, `Meetings`. No change to
  `hlin_widget_ui::Client` was needed.
- **Deploys** is the first widget to stream through the trait:
  `widget.stream(Request::get("/api/deploys/log"))`, read chunk by chunk. On
  its own page that is `Direct`'s streamed same-origin `fetch` of its own
  `/api/deploys/log` (a `ReadableStream` reader, cancelled when dropped); in
  its module, the bridge's `fetch_stream`. The SDK's end reasons arrive as
  `Ended`; `Ended::Gone` (the widget itself going) stops following, every
  other end is said in words and followed again in three seconds from
  `?after=`, as before.
- **Converter** asks for nothing with either client: its rule test now reads
  the components, `units.rs`, the module and the own UI for any request call,
  `.stream(` included. It keeps what was typed through `widget.restored()` and
  `widget.on_suspend(…)`; its own page is never suspended.
- **Server tests** (1076 in `angreal test all`, 7 new): shoutbox, reactions,
  bookmarks, picker and meetings each write through their own `/api/` (as the
  local user) and through `/hlin/api/` and see one set of data, each person's
  own marked as theirs; oncall's rota the same both ways; the deploy log
  streamed at its own `/api/`, line for line the log Hlin reads.
- **e2e twenty** (8 passed, twice): the eight in `OWN_UI`; a new test draws
  each at its own root with nothing refused, checks the converter makes no
  request as a value is typed, watches three more deploy lines arrive on its
  own page (2.9 s, screenshot `216-twenty-deploys-own-ui-streaming.png`; the
  module's own streaming test still passes), and a shout said on its own page
  reaches its module on "Twenty" (312 ms). **twenty-measure**: 4 passed.
- **A shell race, fixed** (`hlin-ui` `frame.rs`, `seen`). "A shout reaches a
  second browser" failed on the base commit too (2 of 2 with the counter test
  before it, 1 of 3 full runs): a panel waiting for room at the frame budget
  stayed blank when the frame it waited on came back into view and cancelled
  its suspension, because nothing asked for room again until the waiting
  panel next entered the viewport. Cancelling a suspension now calls
  `mount_waiting()`, as a frame leaving already did. After it, that pair and
  both full runs passed.
- **For whoever lands second** of this and [[HLIN-T-0096]]: once both are in,
  no widget uses `hlin-widget-module`'s `legacy.rs`; delete it, its words for
  the SDK's types, `hlin_widget_support::run` and `Widget::built`, as the
  support crate's docs say. Left in place here because HLIN-T-0096's widgets
  still used it when this was written.
