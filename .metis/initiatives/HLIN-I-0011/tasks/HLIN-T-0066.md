---
id: mount-a-module-in-a-sandboxed
level: task
title: "Mount a module in a sandboxed frame, and keep its panel's state honest"
short_code: "HLIN-T-0066"
created_at: 2026-09-25T00:01:03.997781+00:00
updated_at: 2026-09-25T00:01:03.997781+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


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
