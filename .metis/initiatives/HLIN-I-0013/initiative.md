---
id: twenty-platforms-deployed-the-way
level: initiative
title: "Twenty platforms, deployed the way we intend"
short_code: "HLIN-I-0013"
created_at: 2026-09-25T23:54:57.213964+00:00
updated_at: 2026-09-26T05:38:26.383635+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: M
initiative_id: twenty-platforms-deployed-the-way
---

# Twenty platforms, deployed the way we intend

## Context

[[HLIN-I-0012]] put twenty widget platforms on one surface, each a process on
a port of the developer's machine. The intended deployment is different, and
the owner wants the demo to match it (2026-09-25):

- Each platform lives at the root of its own host (`widget.comp.net`), where
  its **core UI** is served at `/`: a client-side Leptos app built by Trunk,
  embedded, behind a catch-all fallback. That is how kairos and skadi serve
  theirs today.
- Hlin's surface for a platform lives in a separate **`/hlin/`** subtree on
  that host: the manifest, the module's Trunk build, the data routes it calls,
  and its event stream. The shell's `base_url` for it is
  `https://widget.comp.net/hlin`, which the shell already supports (it appends
  the manifest path and every declared prefix to `base_url`).
- The shell is its own host (`hlin.comp.net`), a release build, signing people
  in through an identity provider.

## Decisions (owner, 2026-09-25)

| Question | Decision |
|---|---|
| What a widget container serves at its root | **A tiny core-UI SPA** behind a catch-all fallback, as kairos and skadi do, so the `/hlin` carve-out is proved against a real fallback |
| Sign-in | **Dex, as the collab demo does**, with alice, bob and carol; the shell a release build |
| Names | **Mirror comp.net on the compose network**: `clock.comp.test` to `meetings.comp.test`, `hlin.comp.test`; only the shell's port published |
| What the Hlin module is | **A re-export of the base SPA's components.** Each widget has one components crate; its core UI at `/` mounts them with a client that calls the platform's own `/api/`; its Hlin module mounts the very same components with a client that goes through the bridge. The module has no UI of its own |

## Goals & Non-Goals

**Goals:**
- Twenty containers, one per widget, each serving its core UI at `/` and
  Hlin under `/hlin/`, with unknown `/hlin/*` paths answered 404, never with
  the core UI's `index.html`.
- One compose file bringing up Postgres, Dex, the shell and the twenty, with
  release builds throughout and images built once in a shared builder stage.
- The published "Twenty" surface, the twenty browser suite and the twenty
  measurement passing against the containers, with a widget container
  stopped and started standing in for a platform going down and coming back.

**Non-Goals:**
- TLS, a reverse proxy, or an ingress. The browser talks only to the shell,
  on a published port; the platforms are reached only on the compose network.
- Changing how widgets authorise. Hlin's routes verify `hlin-token`, as now.
- Retiring the process-based `--with twenty` flavour; it stays for quick work.

## Implementation Plan

| # | Task | Depends on |
|---|---|---|
| 1 | [[HLIN-T-0093]] The pattern: a client trait with a direct and a Hlin implementation, a components crate per widget, the base SPA at `/` behind a fallback, the module as a re-export, Hlin under a configurable `/hlin` base, embedding; proved on clock, counter and poll | nothing |
| 2 | [[HLIN-T-0096]] Widgets four to twelve converted | 1 |
| 3 | [[HLIN-T-0097]] Widgets thirteen to twenty converted | 1 |
| 4 | [[HLIN-T-0094]] A builder Dockerfile and a compose file for Postgres, Dex, shell and twenty; a demo flavour that brings it up and publishes "Twenty" | 2, 3 |
| 5 | [[HLIN-T-0095]] The twenty suites and measurement against the containers; results recorded beside [[HLIN-I-0012]]'s | 4 |

## Results

Measured by [[HLIN-T-0095]] with `angreal e2e twenty-measure --against
compose` against `angreal demo up --with twenty-compose`: release
throughout, signed in through Dex as Alice, medians of three, on the Apple M3
Pro (12 cores, 36 GB) of [[HLIN-I-0012]]'s numbers, headless Chromium from
Playwright 1.63, 1280×720, six widgets in view. Beside them, [[HLIN-I-0012]]'s
process-based numbers (release, the shell compressing once):

| | Processes | Containers |
|---|---|---|
| Cold, navigation → six drawn, loopback | 599 ms | 603 ms |
| Cold over 50 Mbit/s, 40 ms → six drawn | 1,091 ms | 1,076 ms |
| Warm → six drawn | 534 ms | 533 ms |
| Cold bytes, first screen | 1.95 MB | 1.94 MB |
| Warm bytes, first screen | 0.05 MB | 0.05 MB |
| JS heap / browser resident memory | 15.1 / 526 MB | 15.6 / 525 MB |
| Frames mounted while scrolling, at the peak | 12 | 12 |
| A counter bump, one browser to another | 77 ms | 93 ms |
| Platform back → open page `ready` by itself | 4.8 s | 4.8 s, then 3.8 s |

The same, within the runs' spread: both are a release shell compressing each
file once, and the containers change only the path to it (Docker Desktop's
port forwarding to the published 8090; the compose bridge from shell to
widget, rather than loopback). A change's trip crosses both and is about
15 ms longer. The twenty suite passes against the containers (9 of 9),
including every widget's own UI at `/` and a 404 for `/hlin/nope`, checked
from inside the compose network; a widget stopped with `docker compose stop`
and started again degrades its panel alone and recovers by itself, twice in
one session. Proving it found that `docker compose stop` took ten seconds,
since no server here handles SIGTERM and PID 1 ignores a signal it has no
handler for; `init: true` in the compose file makes it 0.2 s. Details in
[[HLIN-T-0095]].

## Status Updates

### 2026-09-25 — opened

Opened from the owner's request with the three decisions above.

### 2026-09-25 — the module re-exports the base SPA's components

The owner: the Hlin components should be re-exports of the base SPA's
components. So each widget's core UI is its real UI, not a placeholder, and
the module is a thin mount of the same components with a Hlin client. The
base SPA needs its own data path: each widget serves `/api/` (its own UI) and
`/hlin/api/` (Hlin, verified with `hlin-token`) over the same handlers. The
platforms' own authentication is not what this demo designs, so in the demo
`/api/` acts as a fixed local user, marked demo-only; the containers are not
published, so it is reachable only on the compose network. Converting twenty
widgets is split: the pattern and three widgets, then two batches.

### 2026-09-26 — proved and measured

[[HLIN-T-0095]]: the twenty suites run against the containers (`angreal e2e
twenty --against compose`, `twenty-measure --against compose`), signed in
through Dex; the numbers match the process-based ones (Results, above). The
README says how to run it and that it is the reference for how a platform
serves Hlin.
### 2026-09-26 — completed

All six tasks done. Every widget serves its own UI at `/` behind a catch-all
fallback and Hlin under `/hlin`; every Hlin module is a re-export of the base
SPA's components and a mount. Twenty containers, a release shell and Dex come
up from one compose file (`angreal demo up --with twenty-compose`), and the
twenty suites pass against them signed in through Dex, with a widget
container stopped and started twice as an outage. The containers measure the
same as the processes within run-to-run spread.

The intermittent timeouts were one product bug (a panel waiting for
frame-budget room stayed blank when the frame it waited on came back into
view), fixed in [[HLIN-T-0097]] and now guarded by a test; 104 runs since
without a failure.

Verified on main at close: 1085 Rust tests; collab sign-in 9 and walkthrough
3 after the shared sign-in helper moved; the agents' runs of Aurora (78),
twenty and twenty-measure 20 of 20 each, and both against the containers.
