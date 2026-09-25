---
id: mount-a-module-in-a-sandboxed
level: task
title: "Mount a module in a sandboxed frame, and keep its panel's state honest"
short_code: "HLIN-T-0066"
created_at: 2026-09-25T00:01:03.997781+00:00
updated_at: 2026-09-25T01:36:12.941519+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Mount a module in a sandboxed frame, and keep its panel's state honest

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 6 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Frames*, the envelope,
`init`/`ready`/`heartbeat`, *Panel states* and *Fallback*. The parent end of
the bridge, in `hlin-ui`.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] A panel whose manifest declares `ui` renders a frame at
      `/m/{platform}{entry}#instance`, `sandbox="allow-scripts allow-forms"`
      and nothing more, with an accessible title
- [x] The page keeps a registry from `contentWindow` to platform, panel,
      instance and mount time, and accepts a message only from a registered
      frame; everything else is dropped. Removal from the registry precedes
      removal from the document
- [x] Envelope validation per spec; unknown types and fields ignored
- [x] `init` sent on load; `ready` within 10 s moves the panel to `ready`;
      a major mismatch is `unavailable (malformed)`
- [x] Heartbeat every 2 s while visible, none while hidden; one miss `stale`,
      three `unavailable (unreachable)`; recovery rules as specified
- [x] Fallback to shell-drawn data when the panel declares it
- [x] Frames mount within 200 px of the viewport; the drag shield covers frames
      while a panel is dragged or resized
- [x] `fetch` messages carried to `/p/` and answered with `response`
      (non-streaming), with per-frame `fetches_in_flight` and
      `messages_per_second` enforced in the page
- [x] Unit tests for the registry and state mapping; a browser test with a
      trivial hand-written module (plain JS is fine here) for mount, ready,
      a hang, and fallback
- [x] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0062]], [[HLIN-T-0064]], [[HLIN-T-0065]].
- `crates/hlin-ui/src/` (`grid.rs`, `state.rs`, `app.rs`, a new `frame.rs`
  or `bridge.rs`); wire types beside `hlin-stream` if the SDK should share
  them.
- Built alongside [[HLIN-T-0069]]: each is the other's test.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 (started)

Read the task, HLIN-S-0007 and HLIN-T-0069's summary. Plan:

- **Shell.** `accepted_panels()` stops filtering module-only panels;
  `catalog_panel` always answers, with `kind`/`envelope` optional and a new
  `ui` (`entry`, `bridge`), so the picker and `/api/platforms` both carry it.
  `CatalogPlatform` gains the module limits in force for the platform (the
  page needs `fetches_in_flight`, `messages_per_second` and the byte limits
  for `init.limits`). A module-only panel on a layout is left out of the
  stream's composition entirely (no instance, no fetch, not retired); a panel
  declaring both keeps being fetched so fallback is immediate.
- **Page.** `hlin-ui/src/bridge.rs`, free of the DOM and tested on the host:
  the registry (window handle to platform, panel, instance, mount time), the
  liveness machine (loading, ready, stale, unavailable; `init` resend every
  250 ms, ready timeout, heartbeat misses, visibility), the per-frame
  allowance (`messages_per_second`, `fetches_in_flight`), the `/p/` path
  builder (refuses anything a browser would normalise out of the platform's
  prefix), and the fallback rule. `hlin-ui/src/frame.rs` is the wiring: one
  window `message` listener, one 250 ms tick, iframes created imperatively so
  registry removal can precede document removal.
- **Grid.** Panels rendered with a keyed `<For>` so a frame is not recreated
  on every draft change or stream frame (the old single closure redrew every
  panel on each pointer move, which would reload every iframe). A drag shield
  over the grid while a gesture is held.
- **Sample platform.** `assets = /ui/`, `routes.read = [/api/module/]`, a
  hand-written plain-JS probe module and a silent module, a `whoami` read
  route, and module panels (`module-probe` ui+data, `module-silent` ui+data
  that never says ready, `module-only` ui alone).

### 2026-09-24 (implemented)

**What.** As planned above. `hlin-ui/src/bridge.rs` (25 host tests: the
registry, liveness, fallback, entry statuses, the allowance, the `/p/` address,
refusal statuses and headers), `hlin-ui/src/frame.rs` (the wiring and the
`ModuleFrame` component), the grid rendered with a keyed `<For>`, a drag
shield, a `ModuleUnavailable` body with "Try the module again". Shell: module
panels offered, `ui` and `module_limits` in the catalogue and
`/api/platforms`, module-only panels left out of the stream (two new tests in
`crates/hlin/tests/layouts.rs`, the registry test rewritten). Sample platform
2.2.0: `assets = /ui/`, `routes.read = [/api/module/]`, `/api/module/whoami`,
hand-written `ui/probe` (plain JS: ready, heartbeats, a whoami `fetch`, a
"Stop answering" button and an "Ask twenty at once" button) and `ui/silent`
(never ready), and panels `module-probe` and `module-silent` (ui + data,
drawn from `records-per-second`) and `module-only` (ui alone).
`e2e/tests/modules.spec.js`: 7 browser tests (mount and frame attributes,
`init` received, a `fetch` answered by the platform as the viewer, a
module-only panel, `too_many` past eight in flight, a frame surviving a drag
with the shield over it, stale → unavailable → fallback → retry, and the
ready timeout → fallback).

**Found.**

- *The demo could not host a module at all.* `Config::origin()` defaults to
  `http://localhost:{port}`, but `demo up` prints, and the browser suite opens,
  `http://127.0.0.1:8080`. The module CSP's `frame-ancestors` and the page's
  `frame-src` both name the origin, so every frame was refused. The four
  non-collab demo configurations now set `public_url = "http://127.0.0.1:8080"`
  (collab already did). A person opening `localhost:8080` gets no modules.
- *The old grid redrew every panel on every draft change and stream frame*,
  which would have reloaded every module on each pointer move. Hence the
  keyed `<For>`; a panel's heading and body now redraw from a memo with its
  position set aside, so a drag moves a panel without redrawing it. The drag
  test asserts a JS global set in the frame survives the gesture.

**Decisions.**

- *Frames are created imperatively* in `frame.rs`, not by the view, so one
  function removes the registry entry and then the element. A module given up
  on is torn down there, before the view hears, deferred a turn so a frame's
  own `load` callback never drops itself.
- *The page asks for the entry itself* beside the frame, so a 4xx is
  `malformed` and a 5xx/no answer `unreachable` at once, instead of both
  waiting out the ready timeout. The frame's own request revalidates it.
- *No `loading="lazy"`*: the page already mounts within 200 px, and a load the
  browser defers would be a ready timeout the module did not earn.
- *Ready timeout* runs from the load event, or from the mount when the
  document never loads, so a frame is never `loading` forever.
- *Heartbeats start two seconds after `ready`*; only an echo of the
  outstanding count counts; hiding forgets the outstanding one and showing
  sends the next at once; `unavailable` is terminal until remounted.
- *Recovery from `unavailable`* is a button ("Try the module again"), not a
  loop. Remounting on the platform's next `changed` is HLIN-T-0067's.
- *A panel declaring both ui and data is still fetched by the aggregator*
  while its module runs, so fallback draws at once. A module-only panel is
  absent from the stream entirely (not retired).
- *`CatalogPanel.kind`/`envelope` became optional*; the picker labels a
  module-only panel "module" and hides its kind selector.
- *`fetch` with `stream: true`* is refused with `method` until HLIN-T-0068.
  A path any URL parser would resolve outside `/p/{platform}/` (dot segments
  in any spelling, backslashes, `?`/`#` in the path) is refused in the page
  with `outside_prefix`, because the browser resolves it before the shell
  could check it — `/p/a/../../api/layouts` is the shell's own API.
- *`init.theme` is the default* (light, no tokens) and `context` is recorded
  in the page (`frame::set_context`) but not yet sent on change; both are
  HLIN-T-0067's. The probe uses the system's colours so it is legible
  meanwhile.

**Checks.** `angreal check all` clean; `angreal test all` 671 passed, 0
failed, 3 ignored; `angreal ui build` ok. Against `demo up --with aurora`,
`angreal e2e test`: 30 passed, 10 skipped (open and sign-in suites, and the
non-Aurora component fallback), 0 failed. Against the default flavour the
module suite passes too (26 passed; the four `interaction.spec.js` tests fail
there, as they always have without Aurora). `angreal demo walkthrough`:
everything held.

**For the next tasks.** HLIN-T-0067: `frame::set_context` is the hook for
`context`; `frame::post` sends anything; `handle` in `frame.rs` is where the
module's `set-param`, `set-range`, `navigate`, `changed`, `notice` and `state`
land (currently ignored); `seen` already sends `visibility`; the budget needs
`unmount` plus a held `restored`. HLIN-T-0068: `admit_fetch` refuses
`stream`; `Allowance` counts in-flight fetches and already exempts `pull`.
HLIN-T-0070: `ModuleFrame`/`Mount` take a panel today; a page needs
`init.page = true` and no fallback. Anyone opening the demo must use
`127.0.0.1`, not `localhost`.
