---
id: twenty-platforms-deployed-the-way
level: initiative
title: "Twenty platforms, deployed the way we intend"
short_code: "HLIN-I-0013"
created_at: 2026-09-25T23:54:57.213964+00:00
updated_at: 2026-09-25T23:55:39.305890+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/active"


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
