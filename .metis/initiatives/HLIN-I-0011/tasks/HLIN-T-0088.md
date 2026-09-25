---
id: find-why-a-warm-visit-downloads-a
level: task
title: "Find why a warm visit downloads a module again"
short_code: "HLIN-T-0088"
created_at: 2026-09-25T12:37:22.248049+00:00
updated_at: 2026-09-25T18:45:00.193337+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
initiative_id: HLIN-I-0011
---

# Find why a warm visit downloads a module again

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0084]]: on a warm visit (same browser context, reload),
the kanban widget's 0.6 MB wasm was downloaded again every time, while the
other modules came from cache. Cause unknown.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] The cause found and written down (cache headers from the platform, the
      asset proxy's `Cache-Control`/`ETag` handling, Trunk's hashed file
      names, or the module itself)
- [x] Fixed where it belongs, with a test that a second visit fetches no
      module wasm that has not changed
- [x] Warm bytes for "Twenty" re-measured and recorded in [[HLIN-I-0012]]

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.

### 2026-09-25 — the cause, and fixed

**Cause: Trunk's hashed file names, as the widget file server read them.**
Kanban's build was `hlin-widget-kanban-module-ef8fd81f1008991_bg.wasm`:
fifteen hex digits, where every other module has sixteen. Trunk writes its
`u64` content hash with `{:x}`, which drops leading zeros, so one build in
sixteen gets a fifteen-digit hash (one in 256, fourteen). The widgets' file
server (`hlin-widget-support`, `files.rs`) took a name as hashed only with
exactly sixteen digits, so kanban's wasm, JS and nothing else were served
`no-cache` rather than `public, max-age=31536000, immutable`. The shell
passed `no-cache` on, as it should for anything not `immutable`. A
`no-cache` file is revalidated on every use, and the file server sent no
`ETag` or `Last-Modified`, so there was nothing to revalidate with: every
visit was a full 200. The platform's headers, compared on the demo:
`dice`'s wasm `immutable`, kanban's `no-cache`, neither with a validator.
Not the shell's ETag passthrough (it passes validators back; there were
none), and not the module.

**Fix, where it belongs** (the file server, and its two copies in the
checklist and feed samples):

- A hash is eight to sixteen hex digits. Fewer than eight is one build in
  four billion, and the floor keeps a short all-hex word at the end of a
  crate name (`-feed`, `-cafe`) from passing for one.
- Every file carries a strong `ETag` (the first 128 bits of its SHA-256)
  and a matching `If-None-Match` (compared weakly, `*` and lists included)
  gets `304` with no body. So a name misjudged as unhashed costs a round
  trip, not the file; and so do the entry documents and `boot.js`, which are
  `no-cache` by design and were re-downloaded on every mount.

Tests: `files.rs` has kanban's names verbatim, the `-feed`/`-cafe` negatives,
and a second request for each kind of file with its `ETag` (plain, `W/`, in
a list) answered `304` with the same headers. `twenty-measure` now asserts
that a warm visit fetched no module `.wasm` with a body, and passed on all
three runs.

**Warm bytes for "Twenty"** (with [[HLIN-T-0086]]'s compression): **0.72 MB
→ 0.05 MB**, first screen, median of three. Of the 54 `/m/` requests a warm
visit makes, none is a wasm or JS download; what remains is entry
documents answered 304 and stylesheets, which Chromium re-reads from its
cache (Playwright reports them with no body).
