---
id: the-widget-support-crate-the
level: task
title: "The widget support crate, the twenty flavour, and the first three widgets"
short_code: "HLIN-T-0080"
created_at: 2026-09-25T02:42:59.708560+00:00
updated_at: 2026-09-25T02:43:42.931217+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# The widget support crate, the twenty flavour, and the first three widgets

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 1 of [[HLIN-I-0012]]: everything the other widget tasks copy from,
proved on three widgets.

## Acceptance Criteria

## Acceptance Criteria

- [ ] `crates/widgets/hlin-widget-support`: a small library a widget server
      uses to serve its manifest (built from a short description), verify
      `hlin-identity` tokens (bound on writes), keep recent idempotency keys,
      run its event stream, and serve `module/dist` under its `assets` prefix
      with `immutable` for hashed files. Documented as the answer to "what
      does a platform need to host a module"
- [ ] Widgets 1 to 3 (`clock`, `counter`, `poll`) as full crates
- [ ] A way to add a widget that the next tasks follow: a short section in
      the support crate's docs, and an entry in one list the demo reads
- [ ] `angreal demo up --with twenty`: builds every widget's module (in
      parallel, skipping unchanged ones), starts each widget on its own port
      from 8201, starts the shell on `dev` sign-in with a config listing them
      all, and publishes "Twenty" as a scrolling surface in a sensible grid.
      With three widgets it publishes three; each later task grows the list
- [ ] `angreal demo down` stops them all (and remember `angreal db up` after)
- [ ] A browser test: the published surface shows each widget `ready`, and
      the counter and poll change in a second browser context without reload
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

- Copy what works from `crates/hlin-sample-checklist` and
  `crates/hlin-sample-feed` (their `module/` setup with Trunk, the
  `boot.js` loader swap for the module CSP, `--module-dir`, the Aurora token
  mapping in `module.css`) into the support crate or a template, rather than
  inventing again.
- Workspace members glob `crates/*`; add `crates/widgets/*` and
  `crates/widgets/*/module`.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.
