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

## Results

Measured by [[HLIN-T-0084]] with `angreal e2e twenty-measure` against `angreal
demo up --with twenty --release`: medians of three, on an Apple M3 Pro (12
cores, 36 GB, macOS 26.6.2), headless Chromium from Playwright 1.63, all on
loopback, with another build running on the same machine. The viewport is
1280×720, so six widgets are in view at the top and nine frames mounted. The
details, and how each number is taken, are in the task.

| | Median | NFR-1.1 |
|---|---|---|
| Cold: navigation → six in view `ready` / drawn | 572 ms / 588 ms | six interactive within 3 s cold: **met** |
| Cold over 50 Mbit/s, 40 ms latency | 1,704 ms / 1,749 ms | **met** |
| Warm (same context): `ready` / drawn | 528 ms / 535 ms | a cached module within 1 s: **met** |
| Cold bytes, first screen | 7.73 MB (modules 5.69, shell 2.02) | |
| Warm bytes, first screen | 0.72 MB | |
| All twenty modules' assets | 12.53 MB as sent, 3.98 MB gzipped; **nothing is compressed** | |
| JS heap | 15.1 MB (19.0 MB after a scroll) | |
| Browser resident memory | 526 MB with the surface open, 615 MB after a scroll; 276 MB for an empty page | |
| Frames mounted while scrolling | 12 settled; **18 at the peak**, 12 since [[HLIN-T-0085]] | budget is 12 |
| A counter bump, one browser to another | 77 ms | |

**Since: compression, `wasm-opt`, and the warm re-download**
([[HLIN-T-0086]], [[HLIN-T-0088]]). The shell now compresses module assets
and its own frontend (brotli or gzip), release builds run `wasm-opt -Oz`,
and kanban's wasm is no longer fetched again on every visit (a fifteen-digit
Trunk hash was taken for no hash, and served `no-cache` with no validator).
Re-measured the same way:

| | Before ([[HLIN-T-0084]]) | After |
|---|---|---|
| Cold bytes, first screen (9 frames) | 7.73 MB (modules 5.69, shell 2.02) | **2.27 MB** (modules 1.70, shell 0.56) |
| Every module asset once (20 modules), as sent | 12.53 MB | **2.00 MB** (11.32 MB uncompressed) |
| Shell frontend, as sent | 2.01 MB | **0.54 MB** (1.78 MB uncompressed) |
| Warm bytes, first screen | 0.72 MB | **0.05 MB** |
| Cold, navigation → six drawn (first content), loopback | 588 ms | 817 ms (see below) |
| Cold over 50 Mbit/s, 40 ms → six drawn | 1,749 ms | **1,373 ms** |
| Warm → six drawn | 535 ms | 537 ms |

Loopback is slower because the demo's shell is a debug binary compressing
as it goes (brotli there is about ten times slower than optimised: 223 ms
against 22 ms for the frontend's wasm); over a real link the smaller bytes
win by 380 ms. Details in [[HLIN-T-0086]].

**Proofs.** A scrolled-away converter keeps its value and a stopwatch keeps
running; a half-written note was lost until the notes module was given a
`suspend` hook, and now keeps its draft. Killing one widget's platform moves
no other panel, and the rest keep working; a page opened while it is down
falls back to Hlin-drawn data. But a page already open keeps that panel
`ready`, with the module saying it cannot reach its platform, because the
heartbeat never reaches the platform. After a restart nothing recovers by
itself (by design: on the platform's next change, or when a person asks);
asked, the panel is back in 0.85 s.

**Since, on 2026-09-25** (same machine and runner, three runs). With a frame
counted until it has left the document and the SDK answering `suspend` at
once ([[HLIN-T-0085]]), the peak while scrolling is **12** in every run, and
the test asserts it. With a platform's reach counted and its stream's return
treated as a change ([[HLIN-T-0087]]): after dice is killed, a roll and one
*Try again* on the open page make three `unreachable` answers in a row, and
the panel is `stale` 0.19 s after the kill; nothing else moves. After `demo
restart dice`, with nobody touching anything, the open page's panel is
`ready` with its table drawn 4.8 s later, and the page opened while it was
down (fallen back) remounts its module and is `ready` at the same moment.

**Does twenty fit the bet in [[HLIN-A-0014]]?** In time, yes, with room: a
twenty-widget surface draws its first screen in 0.6 s locally and under 2 s
over a good link. In weight, only just. Every module carries its own Leptos
and SDK (0.54 to 0.63 MB, nothing shared between frames), and every mounted
frame costs 20 to 22 MB of renderer memory, so the budget of twelve is about
265 MB. To make it comfortable rather than acceptable: compress what the
shell serves (about 3× fewer bytes, the largest single win); run `wasm-opt`
on modules; count frames being suspended against the budget, or have the SDK
answer `suspend` at once; and let a module whose read failed try again. For
[[HLIN-I-0011]], not changes made here.

## Status Updates

### 2026-09-24 — opened

Opened from the owner's request, with the three decisions above.
