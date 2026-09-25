---
id: each-widget-serves-its-own-ui-at
level: task
title: "Each widget serves its own UI at the root and Hlin under /hlin"
short_code: "HLIN-T-0093"
created_at: 2026-09-25T23:54:58.370523+00:00
updated_at: 2026-09-25T23:55:39.331749+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0013
---

# Each widget serves its own UI at the root and Hlin under /hlin

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Make every widget serve the way a real platform will, and set the pattern the
rest copy: one set of Leptos components per widget, mounted twice. At `/`,
behind a catch-all SPA fallback, the platform's own core UI mounts them with a
client that calls the platform's own `/api/`. Under `/hlin/`, the Hlin module
is a **re-export of those same components** (owner, 2026-09-25) mounted with a
client that goes through the bridge. The module has no UI of its own.

## Acceptance Criteria

- [ ] A small client trait the components are written against (load, send,
      the stream of changes, read-only, and whatever else the widgets use from
      `hlin-widget-module` today), with two implementations: a **direct**
      client (same-origin `fetch` to `/api/…`; changes from the platform's own
      event stream or a poll) and a **Hlin** client (the SDK bridge, as
      `hlin-widget-module` does now). The components depend on neither
      transport
- [ ] For clock, counter and poll: a components crate holding every
      component; a core-UI SPA crate that mounts them with the direct client;
      a module crate that is `pub use` of the components plus a mount with the
      Hlin client, and nothing else
- [ ] Each widget serves its core UI at `/` behind a catch-all fallback, as
      kairos's `spa_fallback` does, and Hlin's whole surface (manifest at
      `/hlin/.well-known/hlin.json`, module assets, data routes, event stream)
      under a base path, `/hlin` by default and configurable
- [ ] Two route trees over the same handlers: `/api/…` for its own UI, which
      in the demo acts as a fixed local user and says so plainly (a flag, off
      by default, with a warning logged at start), and `/hlin/api/…`
      verified with `hlin-token` as now
- [ ] Unknown paths under `/hlin/` answer 404, never `index.html`, tested for
      a missing module file, a missing route and the manifest path
- [ ] Both `dist`s can be embedded in the binary (a feature, as kairos's
      `embed-web`), and still read from disk in development
- [ ] The process-based `--with twenty` flavour uses `/hlin` base URLs; the
      other seventeen widgets keep working unchanged until [[HLIN-T-0096]] and
      [[HLIN-T-0097]] convert them (make both shapes work in the meantime);
      `angreal e2e twenty` and `twenty-measure` pass
- [ ] The support crates' docs say, for a real platform team, exactly what to
      add: the components/SPA/module split, the client trait, the `/hlin` nest
      before the fallback, the 404 rule, the module's Trunk settings
      (`public_url = "./"`, the `boot.js` loader swap, `data-wasm-opt`),
      cache headers, and the Leptos 0.8 note. Short and exact: the next two
      tasks follow it for seventeen widgets
- [ ] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created; reshaped the same day for the owner's direction that the module
re-exports the base SPA's components.
