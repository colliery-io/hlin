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

Make every widget serve the way a real platform will: its own core UI at `/`
behind a catch-all SPA fallback, and Hlin's surface under a `/hlin` base.
This is the pattern kairos and skadi need to adopt, so the widget crates
become its reference.

## Acceptance Criteria

## Acceptance Criteria

- [ ] `hlin-widget-support` serves Hlin's whole surface (manifest at
      `/hlin/.well-known/hlin.json`, module assets, data routes, event stream)
      under a base path, `/hlin` by default and configurable
- [ ] Each widget serves a tiny core-UI SPA at `/` (one shared Leptos app,
      Trunk-built, that shows the widget's name and says it is the platform's
      own frontend is enough), behind a catch-all fallback returning
      `index.html` for unknown paths, as kairos's `spa_fallback` does
- [ ] Unknown paths under `/hlin/` answer 404, never `index.html`, with a test
      for each kind (a missing module file, a missing route, the manifest
      path on a platform with none)
- [ ] Module and core-UI `dist` directories can be embedded in the binary
      (a feature, as kairos's `embed-web`), and still read from disk in
      development (`--module-dir`)
- [ ] The process-based `--with twenty` flavour uses `base_url` with `/hlin`,
      and `angreal e2e twenty` and `twenty-measure` still pass
- [ ] The support crate's docs say, for a real platform team, exactly what to
      add: the `/hlin` nest before the fallback, the 404 rule, the module
      crate's Trunk settings (`public_url = "./"`, the `boot.js` loader swap,
      `data-wasm-opt`), cache headers, and the Leptos 0.8 note
- [ ] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created. Not started.
