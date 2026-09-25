---
id: mount-a-module-in-a-sandboxed
level: task
title: "Mount a module in a sandboxed frame, and keep its panel's state honest"
short_code: "HLIN-T-0066"
created_at: 2026-09-25T00:01:03.997781+00:00
updated_at: 2026-09-25T00:54:23.822829+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


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

- [ ] A panel whose manifest declares `ui` renders a frame at
      `/m/{platform}{entry}#instance`, `sandbox="allow-scripts allow-forms"`
      and nothing more, with an accessible title
- [ ] The page keeps a registry from `contentWindow` to platform, panel,
      instance and mount time, and accepts a message only from a registered
      frame; everything else is dropped. Removal from the registry precedes
      removal from the document
- [ ] Envelope validation per spec; unknown types and fields ignored
- [ ] `init` sent on load; `ready` within 10 s moves the panel to `ready`;
      a major mismatch is `unavailable (malformed)`
- [ ] Heartbeat every 2 s while visible, none while hidden; one miss `stale`,
      three `unavailable (unreachable)`; recovery rules as specified
- [ ] Fallback to shell-drawn data when the panel declares it
- [ ] Frames mount within 200 px of the viewport; the drag shield covers frames
      while a panel is dragged or resized
- [ ] `fetch` messages carried to `/p/` and answered with `response`
      (non-streaming), with per-frame `fetches_in_flight` and
      `messages_per_second` enforced in the page
- [ ] Unit tests for the registry and state mapping; a browser test with a
      trivial hand-written module (plain JS is fine here) for mount, ready,
      a hang, and fallback
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass

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
