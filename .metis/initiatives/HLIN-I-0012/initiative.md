---
id: twenty-platforms-on-one-surface
level: initiative
title: "Twenty platforms on one surface"
short_code: "HLIN-I-0012"
created_at: 2026-09-25T02:41:04.680174+00:00
updated_at: 2026-09-25T02:43:42.046537+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/active"


exit_criteria_met: false
estimated_complexity: L
initiative_id: twenty-platforms-on-one-surface
---

# Twenty platforms on one surface

## Context

[[HLIN-A-0014]] bet that sandboxed modules stay "light and consistent enough
to feel like one product", and said to watch load time per surface and
visual drift in the first adoptions. The demos so far put two or three
modules on a surface. Nothing has tested the bet at the scale the vision
describes: a dozen or more independent platforms on one page.

The owner asked for a demo with twenty separate platform widgets published to
one shell. Twenty is past the frame budget (12 mounted per surface,
[[HLIN-S-0007]]), so it exercises that too.

## Decisions (owner, 2026-09-24)

| # | Question | Decision |
|---|---|---|
| 1 | What it proves | **Variety and scale.** Twenty genuinely different small widgets, each its own platform, and measured numbers for load, memory and the budget |
| 2 | How independent | **Twenty separate crates.** Each its own server, module and rules. Scaffolding any real platform would share (manifest serving, token verification, event stream, asset serving, idempotency) lives in one small support crate, as `hlin-identity` is shared today |
| 3 | Layout | **A scrolling surface.** Taller than the screen; roughly eight to twelve in view; scrolling unmounts what leaves view (with `suspend`/`state`) and remounts what arrives |

## Goals & Non-Goals

**Goals:**
- Twenty platforms, twenty crates, twenty processes, twenty modules, each a
  different widget, published as one surface in a new demo flavour.
- Every widget themed by the shell, so drift is visible if it exists.
- Numbers, recorded: cold and warm time to every widget `ready`, page memory,
  mounted-frame count while scrolling, and how a change fans out.
- Browser tests that all twenty reach `ready`, the budget holds while
  scrolling, and a widget whose platform is killed degrades alone.

**Non-Goals:**
- Widgets anyone would ship. They are small on purpose.
- Changing the budget or limits. If the numbers argue for it, that is a
  finding for [[HLIN-I-0011]], not a change made here.

## The twenty

| # | Widget | Shows | Shared between people |
|---|---|---|---|
| 1 | clock | World clocks | no |
| 2 | counter | A number anyone can bump | yes |
| 3 | poll | One question, votes | yes |
| 4 | notes | One shared sticky note | yes |
| 5 | dice | Roll, with recent history | yes |
| 6 | stopwatch | A per-person stopwatch | no |
| 7 | quote | A quote, rotated | no |
| 8 | sparkline | A synthetic metric over the time range | no |
| 9 | kanban | One column of cards | yes |
| 10 | status | A service health light | no |
| 11 | weather | Synthetic weather for a few cities | no |
| 12 | pomodoro | A focus timer | no |
| 13 | shoutbox | A tiny chat | yes |
| 14 | reactions | Emoji counters | yes |
| 15 | bookmarks | Shared links | yes |
| 16 | oncall | Who is on call this week | no |
| 17 | picker | Pick someone at random from the team | yes |
| 18 | deploys | A deploy log, streamed | no |
| 19 | converter | Unit converter, entirely local | no |
| 20 | meetings | Today's meetings | no |

## Architecture

- `crates/widgets/hlin-widget-support`: shared platform scaffolding (serve a
  manifest built from a small description, verify `hlin-identity` tokens
  with bound tokens on writes, event stream, serve `module/dist`,
  idempotency). A reference for "what a platform needs", in one place.
- `crates/widgets/<name>/`: one crate per widget, with `src/` (the server:
  its routes and rules) and `module/` (its Leptos module on `hlin-module`).
- Ports 8201 to 8220. A `twenty` flavour of `angreal demo up` builds the
  modules (in parallel), starts the twenty processes and the shell, and
  publishes "Twenty" as a scrolling surface.
- The shell's own configuration lists twenty platforms on `hlin-token`, with
  `dev` sign-in (the collab flavour already proves `oidc`).

## Implementation Plan

| # | Task | Depends on |
|---|---|---|
| 1 | Support crate, the `twenty` flavour, the published surface, and widgets 1 to 3 as the template | [[HLIN-I-0011]] modules as they are |
| 2 | Widgets 4 to 10 | 1 |
| 3 | Widgets 11 to 17 | 1 |
| 4 | Widgets 18 to 20 (the deploy log streams, so after [[HLIN-T-0068]]) | 1, HLIN-T-0068 |
| 5 | Measure and prove: all twenty ready, budget on scroll, one platform killed, the numbers written down | 2, 3, 4 |

## Status Updates

### 2026-09-24 — opened

Opened from the owner's request, with the three decisions above.
