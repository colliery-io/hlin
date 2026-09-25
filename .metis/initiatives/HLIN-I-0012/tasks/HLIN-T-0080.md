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

- [x] `crates/widgets/hlin-widget-support`: a small library a widget server
      uses to serve its manifest (built from a short description), verify
      `hlin-identity` tokens (bound on writes), keep recent idempotency keys,
      run its event stream, and serve `module/dist` under its `assets` prefix
      with `immutable` for hashed files. Documented as the answer to "what
      does a platform need to host a module"
- [x] Widgets 1 to 3 (`clock`, `counter`, `poll`) as full crates
- [x] A way to add a widget that the next tasks follow: a short section in
      the support crate's docs, and an entry in one list the demo reads
- [x] `angreal demo up --with twenty`: builds every widget's module (in
      parallel, skipping unchanged ones), starts each widget on its own port
      from 8201, starts the shell on `dev` sign-in with a config listing them
      all, and publishes "Twenty" as a scrolling surface in a sensible grid.
      With three widgets it publishes three; each later task grows the list
- [x] `angreal demo down` stops them all (and remember `angreal db up` after)
- [x] A browser test: the published surface shows each widget `ready`, and
      the counter and poll change in a second browser context without reload
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

### 2026-09-24 (implemented)

**What.**

- `crates/widgets/hlin-widget-support` (server side). A widget is a
  `Widget<S>` description (panel key, name, icon, title, description,
  `shared`, an optional `Fallback { kind, envelope, refresh_ms, data }`, and
  where its module is built) from which the manifest is built: `assets = /ui/`,
  `ui.entry = /ui/{panel}/index.html`, read and write routes `/api/`,
  `events = api/events`, fallback `data = api/panels/{panel}`, `pushed` =
  `shared`. `Widget::defects` validates it with `hlin-manifest`; `serve`
  refuses to start a widget it objects to. `Platform<S>` holds the state and
  the idempotency memory behind one lock; handlers take `Viewer(claims)` to
  read and the `Write` extractor to write (bound token and an acceptable
  `Idempotency-Key`, both refused before the handler runs), and answer
  through `Platform::read` / `Platform::write`, which replays repeats (marked
  `Idempotency-Replayed`), 422s a reused key, remembers only writes that
  happened, and announces `changed` on the stream after the lock is released.
  `router()` adds manifest, health, events (15 s heartbeat), fallback and
  module files (`ModuleFiles`, the feed's, now skipping dotfiles).
  `run(widget, state, api)` is a widget's whole `main`; `Common` is the
  flags (`--name --port --bind --shell-keys --shell-issuer --module-dir`) for
  a widget that wants more of its own. A `testing` feature (a widget's
  dev-dependency) runs a widget on a socket with a real `Issuer` and calls it
  as the shell does, including `fallback()` (parsed as the shell would) and
  `listen()` on the event stream. The crate docs are the answer to "what does
  a platform need to host a module", and hold *Adding a widget*.
- `crates/widgets/hlin-widget-module` (module side): `start(panel, app)`
  (connect with `wasm_bindgen_futures::spawn_local`, mount, `ready`),
  `Widget::load::<T>` (fetch, refetch on context / `changed` for the panel /
  own writes, latest answer wins), `Widget::send` (an `Attempt`; retry offered
  only for network failures; `changed` after success; refusals in the
  platform's words), and `widget.css`, written only against `--hlin-*`
  tokens with fallbacks, injected as an inline `<style>`.
- Widgets 1 to 3, each `src/` (server, rules as plain functions on the
  state), `module/` (Trunk, `boot.js` loader, `module.css`), `tests/rules.rs`:
  - `clock` (8201): per-person cities from a known twelve, default London /
    New York / Tokyo, at most six, at least one; offsets from `chrono-tz`
    (DST correct), so the module ticks with the browser clock plus an offset
    and carries no zone data. Not shared (`pushed` false); still announces.
    Fallback `table`, `refresh_ms` 30 s.
  - `counter` (8202): bump by ±1, never below zero, remembers who. Shared.
    Fallback `stat`.
  - `poll` (8203): one question, one vote per `sub`, revote replaces,
    take-back. Shared. Fallback `table`.
- `angreal demo up --with twenty`: `WIDGETS` in `.angreal/task_demo.py` is
  the one list. `demo/hlin-twenty.toml` (dev sign-in, `public_url`
  127.0.0.1:8080) plus one `[[platforms]]` per entry is written to
  `demo/state/hlin-twenty.toml` and run. Modules: a content hash of each
  module's sources plus `hlin-widget-module`, `hlin-module/src`,
  `hlin-bridge/src` and `Cargo.lock`, stamped in `dist/.built-from`; stale
  ones are compiled by one `cargo build --target wasm32-unknown-unknown -p …`
  (shared deps, all cores), then one `trunk build` each, all at once (they
  find the compile done). "Twenty" is published through the API as the dev
  user, replacing any existing one: 3 across, `w=4 h=5`, in list order.
  `demo down` stops every widget. `angreal e2e test` now refuses against the
  twenty demo, and `angreal e2e twenty` refuses against anything else.
- `e2e/tests/twenty.spec.js`: every panel on "Twenty" scrolled into view and
  `data-module=ready`; two contexts, counter bump and a poll vote reach the
  second without a reload (reloads counted).
- Workspace members: `crates/hlin*`, `crates/hlin*/module`,
  `crates/widgets/*`, `crates/widgets/*/module`. `crates/*` would match the
  `crates/widgets` folder itself (no Cargo.toml) and fail, and `exclude`
  takes its children with it.

**Decisions.**

- A second shared crate, `hlin-widget-module`, for the module side. The
  support crate is server-only (axum, tokio); twenty copies of the feed's
  `api.rs` words and refetch effect would be the drift this initiative is
  looking for.
- No Aurora in widget modules: `widget.css` against `--hlin-*` directly,
  so the widgets are themed only by the shell, as the task asks, and smaller.
  The body is `--hlin-raised`, not `--hlin-surface`: the frame fills a panel,
  and with surface there was a visible seam under the panel header.
- One panel per widget, key = the widget's name = its default platform id.
- `--port` has no default: the list is the only place a port is written.
- Modules are debug builds, like the collab ones: about 2.7 MB of wasm each.
  Twenty on a page is ~54 MB of wasm; task 5 should decide whether the
  numbers are taken on release builds.

**Build times** (three modules, `demo up`'s module step): cold (no
`target/wasm32-unknown-unknown`, no `dist`) 16.8 s; one module changed
0.7–1.1 s; warm (nothing changed) skipped, the whole `demo up --with twenty`
4.5 s.

**Checks.** `angreal check all` clean. `angreal test all` 778 passed, 0
failed, 3 ignored. Against `demo up --with twenty`, `angreal e2e twenty` 2
passed: 3 widgets ready in ~1.2 s; a bump reached the second browser in
3–82 ms, a vote in 82–185 ms. `angreal e2e test` against it refuses, as
intended.

**Found, not fixed:** nothing in the shell or `hlin-ui`.
