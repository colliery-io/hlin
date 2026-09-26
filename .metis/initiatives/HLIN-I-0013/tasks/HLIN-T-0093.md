---
id: each-widget-serves-its-own-ui-at
level: task
title: "Each widget serves its own UI at the root and Hlin under /hlin"
short_code: "HLIN-T-0093"
created_at: 2026-09-25T23:54:58.370523+00:00
updated_at: 2026-09-26T00:50:46.745599+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
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

## Acceptance Criteria

- [x] A small client trait the components are written against (load, send,
      the stream of changes, read-only, and whatever else the widgets use from
      `hlin-widget-module` today), with two implementations: a **direct**
      client (same-origin `fetch` to `/api/…`; changes from the platform's own
      event stream or a poll) and a **Hlin** client (the SDK bridge, as
      `hlin-widget-module` does now). The components depend on neither
      transport
- [x] For clock, counter and poll: a components crate holding every
      component; a core-UI SPA crate that mounts them with the direct client;
      a module crate that is `pub use` of the components plus a mount with the
      Hlin client, and nothing else
- [x] Each widget serves its core UI at `/` behind a catch-all fallback, as
      kairos's `spa_fallback` does, and Hlin's whole surface (manifest at
      `/hlin/.well-known/hlin.json`, module assets, data routes, event stream)
      under a base path, `/hlin` by default and configurable
- [x] Two route trees over the same handlers: `/api/…` for its own UI, which
      in the demo acts as a fixed local user and says so plainly (a flag, off
      by default, with a warning logged at start), and `/hlin/api/…`
      verified with `hlin-token` as now
- [x] Unknown paths under `/hlin/` answer 404, never `index.html`, tested for
      a missing module file, a missing route and the manifest path
- [x] Both `dist`s can be embedded in the binary (a feature, as kairos's
      `embed-web`), and still read from disk in development
- [x] The process-based `--with twenty` flavour uses `/hlin` base URLs; the
      other seventeen widgets keep working unchanged until [[HLIN-T-0096]] and
      [[HLIN-T-0097]] convert them (make both shapes work in the meantime);
      `angreal e2e twenty` and `twenty-measure` pass
- [x] The support crates' docs say, for a real platform team, exactly what to
      add: the components/SPA/module split, the client trait, the `/hlin` nest
      before the fallback, the 404 rule, the module's Trunk settings
      (`public_url = "./"`, the `boot.js` loader swap, `data-wasm-opt`),
      cache headers, and the Leptos 0.8 note. Short and exact: the next two
      tasks follow it for seventeen widgets
- [x] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created; reshaped the same day for the owner's direction that the module
re-exports the base SPA's components.

### 2026-09-26 — the pattern, on clock, counter and poll

- **Crates.** `hlin-widget-ui` (new) is transport-agnostic: the `Client`
  trait (`fetch`, `attempt`, `stream`, `changes`, `announce`, `read_only`,
  `visible`, `time_range`, `restored`, `on_suspend`), its own `Request` and
  `Answer` so components never see the SDK, the `Widget` handle written once
  over the trait (`load`, `send`, `send_then`, `reload`, `loaded_view`,
  refusal words, *Try again*), and the shared stylesheet. Its `direct`
  feature is the core UI's client: same-origin `fetch` to `/api/`, changes
  from `EventSource("/api/events")`, a reopen after a drop counting as a
  change. `hlin-widget-module` gained `Hlin` (the bridge client) and
  `mount(panel, Component)`; its old `start`/`Widget` stay, as `legacy.rs`,
  for the seventeen until they are converted.
- **Per widget:** `components/` (every component, `style.css` inlined),
  `ui/` (`direct::mount(<Name>)`, an ordinary Trunk site), `module/`
  (`pub use` of the components and `hlin_widget_module::mount`), and the
  server's `main` calling `run_with(…, Builds { ui: dist!("ui/dist"),
  module: dist!("module/dist") })`.
- **Server.** `site()` nests Hlin's routes under `--hlin-base` (default
  `/hlin`) with a 404 fallback, merges `/api/` for the widget's own UI over
  the same handlers, and falls back to the UI (`index.html` for a route, 404
  for a missing file, 404 for anything under the base, `/api` or
  `/.well-known`). `Viewer` and `Write` accept a local user put on the
  request by the `/api/` layer only; `--local-user` is off by default (then
  `/api/` is 401 in words) and warned about at start. `dist!` embeds a build
  under a widget's `embed` feature (rust-embed, `crate_path` through the
  support crate). The seventeen unconverted widgets serve Hlin under `/hlin`
  too, with a placeholder page at `/`.
- **Demo.** `--with twenty` uses `/hlin` base URLs, builds every module and
  UI in one parallel cargo build plus parallel Trunks, fingerprinted per
  build (a second `up` said "23 widget builds unchanged"), and starts the
  converted three with `--ui-dir` and `--local-user "Local User"`.
- **Verified.** `angreal check all`; `angreal test all` 1069 passed; after
  `demo up --with twenty --release`, `e2e twenty` 7 passed (a new test: the
  three own UIs at their roots, `/nope` their UI and `/hlin/nope` 404, a
  counter bump on its own page reaching its module on "Twenty" in 200 ms and
  back in 26 ms, and a screenshot of the clock's own UI beside its module,
  `215-twenty-clock-own-ui-beside-module.png`); `e2e twenty-measure` 4 passed
  (one earlier run failed once waiting for a middle panel's first content,
  and passed twice after); by hand, the counter's own page with its platform
  killed says "The widget could not be reached", reads back by itself once it
  restarts, and *Try again* resends the bump. The standard suite (`--with
  aurora`) 77 passed.
- **For HLIN-T-0096 and HLIN-T-0097:** `hlin-widget-support`'s crate docs,
  *Converting a widget*, seven steps. Deploys uses `widget.stream`, which
  both clients implement but no converted widget has exercised yet.
