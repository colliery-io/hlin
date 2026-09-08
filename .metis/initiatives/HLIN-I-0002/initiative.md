---
id: running-demo
level: initiative
title: "Running Demo"
short_code: "HLIN-I-0002"
created_at: 2026-09-07T13:56:35.995619+00:00
updated_at: 2026-09-07T15:31:47.404111+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/active"


exit_criteria_met: false
estimated_complexity: XL
initiative_id: running-demo
---

# Running Demo Initiative

## Context

[[HLIN-I-0001]] produced the contracts: four specifications, seven decisions, and two library crates that encode them with tests. It deliberately built nothing that runs. The shell binary prints its tagline and exits. There is no HTTP surface, no discovery loop, no aggregator, no frontend, and nothing draws a panel.

This initiative makes Hlin run, end to end, well enough to be demonstrated to someone who is not reading source. The demo is scoped to prove the vision's central claim rather than to be a product: a person composes a surface from panels that two independent platforms declared, and the shell renders them through one vocabulary, driven by one time picker, degrading honestly when a platform misbehaves.

Every contract the design initiative wrote gets exercised by something real for the first time, which is the point. Specifications written without a running system are guesses with tests.

## Goals & Non-Goals

**The demo, precisely.** Two sample platforms run locally, each serving a manifest and panels across the envelope vocabulary. The shell discovers both. A browser shows a surface with panels from both, updating live over the stream, driven by one time picker. A person opens a picker, adds a panel, drags it into place, resizes it, and it survives a reload. Killing one platform puts its panels through `stale` and `unavailable (unreachable)` while the other's keep working. Redeploying it with a breaking manifest and no major bump produces an operator log line and nothing in the viewer's face. Every data request carries a real signed identity token that the sample platforms verify.

**Goals:**
- A reference sample platform, run twice, that a real platform team can copy.
- The shell as a running service: HTTP host, manifest client, registry loop, aggregator, stream, layout API.
- Identity forwarding implemented per [[HLIN-S-0004]] with a generated dev keypair, and the verification crate platforms will share.
- The rendering interface in `hlin-view`, and a demo design pack that implements it.
- A Leptos frontend: the surface, the time picker, the stream client, the picker, and a drag-and-drop grid.
- A scripted walkthrough that exercises the degradation cases, runnable by one angreal task.

**Non-Goals:**
- A login screen or any real identity provider. A fixed development principal stands in.
- The gallery, publishing, forking, or any sharing UI. The store supports it; the demo does not surface it.
- An operator UI. Violations and defects go to the log.
- The shared design system itself. The demo pack is minimal on purpose and is not it.
- Production deployment, multi-instance operation, TLS, or performance work.
- Any panel a real platform would ship. Sample data is synthetic and looks it.

## Architecture

### Decisions taken in discovery

| Decision | Choice | Why |
|---|---|---|
| Rendering seam | `hlin-view` defines the interface; a design pack implements it; the shell picks a pack. Refines [[HLIN-A-0005]] | `hlin-view` depends on nothing concrete, its totality test stays pure, and Hlin renders before the shared design system exists |
| Composition depth | Full drag-and-drop grid with a picker | The product's central claim is that a person composes the surface; a seeded layout would not demonstrate it |
| Identity | Real Ed25519 tokens from a dev keypair, verified by the sample platforms | Exercises [[HLIN-S-0004]] for real, and the verification code becomes the crate platforms share |
| Identity strategies | The credential is a per-platform strategy behind two traits; `forward-session` is the day-one default where a platform shares the shell's session, `hlin-token` the long-term one. [[HLIN-A-0008]], catalogued in [[HLIN-S-0005]] | A token every platform must adopt before any panel shows is a coordination point. Strategies let the first platform onboard with no change and migrate to the token on its own schedule |

The demo runs its two sample platforms on different strategies, one `forward-session` and one `hlin-token`, so a viewer sees a platform that needed no change beside one that verifies. The demo implements `dev`, `hlin-token`, `forward-session` and `static-bearer`; `oidc`, `trusted-header`, `forward-bearer` and `token-exchange` are specified and land as later tasks.

### New crates

| Crate | Role |
|---|---|
| `hlin-sample-platform` | A small axum binary serving a manifest and synthetic envelopes. Configurable by name and port so several instances run at once. The reference every real platform copies |
| `hlin-identity` | Mint and verify tokens per [[HLIN-S-0004]]: JWKS publication, key rotation, the `X-Hlin-Identity` header, the compile-time dev bypass. Used by the shell to mint and by platforms to verify |
| `hlin-pack-demo` | The demo design pack: Leptos components for the six kinds, hand-rolled SVG charts, minimal styling. Disposable by design |
| `hlin-ui` | The Leptos frontend, compiled to WebAssembly and served by the shell. Separate from `hlin` because a `wasm32` target and a server binary do not share one crate cleanly |

`hlin` itself gains the server: axum routes, the manifest client on `reqwest`, the registry loop, the aggregator, the stream, and the layout API.

### The rendering interface

`hlin-view` gains a trait a pack implements, one method per kind, each taking the envelope and the `RenderPlan` treatment and returning a Leptos view. `plan()` stays where it is and stays total; the pack draws what `plan()` decided. A pack that forgets a kind does not compile. The totality test in `hlin-view` does not change and does not run any pack.

### Frontend shape

Client-side rendered Leptos, served as static assets by the shell. The browser opens the stream, keeps a map from panel instance to its latest frame, and re-renders a panel when its frame changes. Layout edits go to the shell as ordinary requests and the surface re-subscribes.

Server-side rendering with hydration is the better long-term shape and is not needed to prove anything the demo is proving. It is a later initiative.

## Detailed Design

Eight slices, each demoable when it lands. The order is dependency order, and the first two can proceed in parallel.

1. **Sample platform.** `hlin-sample-platform` serves `/.well-known/hlin.json` and one data endpoint per panel, generating synthetic data shaped to the time range and `step` it receives. Panels cover every envelope: a `stat`, two `timeseries` on a shared endpoint (so dedup is visible), a `table`, a `status`, and a `select`-parameterised panel with an options endpoint. A flag serves a deliberately breaking manifest, for the walkthrough. Demo: `curl` the manifest, validate it with `hlin-manifest`.
2. **Identity.** `hlin-identity` mints per-request tokens, publishes a JWKS, and verifies with the caching and `kid`-miss refetch the specification requires. The sample platform verifies on every data request and answers 401 and 403 correctly. Demo: a request without a token is refused; one with a token for the wrong audience is refused.
3. **Shell host and registry.** axum server; a config file naming platforms; a manifest client that fetches with the shell's own identity; the registry loop that validates, diffs with debounce, persists snapshots through the store, and logs violations and rollbacks. Demo: `GET /api/platforms` lists both platforms and their accepted panels; the log shows a violation when the breaking flag is flipped.
4. **Aggregator and stream.** The SSE endpoint per surface and the parameter endpoint, per [[HLIN-S-0003]]: generations, per-principal dedup, coalescing, the state machine, retry with backoff per platform. Demo: `curl` the stream, change the time range, watch generations advance and the shared endpoint fetch once for two panels.
5. **Rendering interface and demo pack.** The trait in `hlin-view`; `hlin-pack-demo` implementing it for six kinds and every treatment. Demo: a test renders each kind in each state to a string and it is not empty.
6. **Frontend.** `hlin-ui`: the surface, the time picker, the stream client, the stream-loss transition, panels drawn through the pack. Demo: the browser shows both platforms' panels moving with the picker.
7. **Composition.** The picker, populated from the registry; add and remove; a drag-and-drop, resizable grid in Leptos with pointer events; layout persistence through the store. Demo: compose a surface, reload, it is still there.
8. **Walkthrough.** `angreal demo up` brings up the database, both sample platforms and the shell; `angreal demo walkthrough` scripts the degradation cases and checks the log. Demo: the whole thing, from nothing, in one command.

Slice 7 is the most expensive item in the initiative by a wide margin, and it is where a real design pack will eventually want to own the grid. The demo grid is written to be replaced, not extended.

## Alternatives Considered

- **Seeded layout instead of composition.** Rejected by decision. It would demonstrate discovery, aggregation and rendering while skipping the claim the product exists to make.
- **Compile-time identity bypass instead of real tokens.** Rejected by decision. The token code gets written either way; writing it now means the demo proves the contract and the verification crate exists for the first platform.
- **A stand-in for the shared design system.** Rejected in favour of the interface. A stand-in impersonates a crate that does not exist and is thrown away; a pack implements an interface that persists.
- **Server-rendered HTML with no WebAssembly.** Rejected. It would be faster to a first picture and would demonstrate nothing about the frontend the shell is meant to have.
- **A JavaScript grid library for drag-and-drop.** Rejected. Pointer-event handling in Leptos is well within reach, keeps the frontend in one language, and avoids a dependency the real pack would inherit.

## Implementation Plan

Decomposed 2026-09-07 into eight tasks, one per slice:

| Task | Slice | Depends on |
|---|---|---|
| [[HLIN-T-0009]] | Sample platform | `hlin-manifest` |
| [[HLIN-T-0010]] | Identity crate, and wiring the sample platform to verify | nothing; the wiring needs T-0009 |
| [[HLIN-T-0011]] | Shell host, manifest client, registry loop, JWKS route | T-0009, T-0010, store |
| [[HLIN-T-0012]] | Aggregator and stream | T-0010, T-0011 |
| [[HLIN-T-0013]] | Rendering interface in `hlin-view`, demo design pack | `hlin-view` only |
| [[HLIN-T-0014]] | Frontend: surface, time picker, stream client | T-0012, T-0013 |
| [[HLIN-T-0015]] | Composition: layout API, picker, drag-and-drop grid | T-0014, T-0011, store |
| [[HLIN-T-0016]] | Walkthrough and one-command demo | everything |

Three tasks can start at once: T-0009, T-0010 and T-0013 share no dependencies. T-0011 follows the first two; T-0012 follows it; T-0014 needs both T-0012 and T-0013; T-0015 and T-0016 are strictly last.

Exit: `angreal demo up` followed by `angreal demo walkthrough` passes from a clean checkout, and a person at a browser can do everything the demo definition above describes.

Judgment calls made here rather than asked, and reversible if wrong: client-side rendering over server-side; pointer events over a grid library; `hlin-ui` as its own crate; a fixed development principal from configuration rather than any login stub.

## Status Updates

### 2026-09-07 — six of eight tasks done

Completed and committed: [[HLIN-T-0009]], [[HLIN-T-0010]], [[HLIN-T-0011]],
[[HLIN-T-0012]], [[HLIN-T-0013]], [[HLIN-T-0014]]. Remaining:
[[HLIN-T-0015]] (composition) and [[HLIN-T-0016]] (walkthrough).

Two shape changes to the plan, both forced by building it:

- **A ninth crate.** `hlin-stream` was split out of the shell to hold the wire
  types, because the browser build cannot take `sqlx`, `tokio` or `axum`. The
  T-0014 risk note called this exactly.
- **Envelope roles.** [[HLIN-S-0002]] gained a role per envelope after a
  totality test found `options.v1` had no non-`raw` kind to pair with. The
  governance rule was right; the matrix was incomplete.

One gap carried forward into T-0016: the frontend has never been rendered in a
browser here, only exercised over HTTP. Looking at it is the first step of the
walkthrough.

### 2026-09-07, later — all eight tasks complete

[[HLIN-T-0015]] and [[HLIN-T-0016]] landed. The initiative's exit criterion is
executable: `angreal demo up` followed by `angreal demo walkthrough` passes, and
a person at a browser can compose a surface, arrange it, and watch it degrade
honestly when a platform misbehaves.

Three things the plan did not anticipate, all of which the plan was right to
expect it would not:

- **A ninth crate.** `hlin-stream`, split out so the browser build takes none of
  `sqlx`, `tokio` or `axum`. The T-0014 risk note named this exactly.
- **Envelope roles.** [[HLIN-S-0002]] gained a role per envelope after a
  totality test found `options.v1` had no non-`raw` kind to pair with.
- **Two defects only running found.** Stored panel selections were never applied
  to a surface, so a reload silently lost a viewer's choice. And
  `Surface::set_registry_state` — the method that makes a withdrawn panel say so
  — was tested, correct, and called from nowhere, so a panel a platform stopped
  offering would have been reported as `malformed` rather than gone. Both are
  fixed and both now have tests.

Checks clean, 263 tests passing, and the walkthrough asserts eight steps against
a running system rather than a mock.

**The one thing to check by hand.** No browser has rendered the frontend at any
point in this initiative: the Chrome extension is not connected in this
environment. The picker, the drag handle, the resize corner and the pointer
capture are unit-tested underneath and unexercised on top. Everything else here
was verified against a running system; this was not, and it is the part a person
sees first. `angreal demo up` and opening the URL is the whole of the check.

The second gap is closed. Docker was out of disk, so `angreal db up` could not
run and the demo was first verified against a Postgres started by hand. After a
`docker system prune` freed 15.5GB, the whole sequence was run again on the
containerised database — `db up`, `demo up`, `walkthrough`, all eight steps,
exit 0 — against an empty store, which also exercised the first-visit path
creating a layout where none existed.

### 2026-09-07, later still — the browser gap is closed, and it was hiding four bugs

[[HLIN-T-0017]] added a Playwright suite: fifteen tests in a real browser
against the running demo, with a screenshot per step. All fifteen pass, the
walkthrough still passes, and 269 Rust tests pass.

The frontend works and now somebody has seen it. It also had four defects that
nothing else in this repository could have found, three of them in the shell:

- **The time picker broke every timeseries panel.** Query values were not
  percent-encoded, so an RFC 3339 `+00:00` reached platforms as a space, they
  answered 400, and the shell reported `malformed` — blaming a platform that
  had done nothing wrong. The product's headline feature, broken for the panels
  it exists to drive.
- **A race in `Surfaces::for_surface`.** The browser posts parameters and opens
  its stream from the same tick, so both requests arrived together after every
  write, both built a surface, and the one the stream was attached to was
  orphaned by the other. Panels sat in loading skeletons forever with nothing in
  any log.
- **Subscribing did not say which generation was in force**, so a browser that
  posted before subscribing never heard the acknowledgement and reported the
  surface as pending for as long as the tab was open.
- **The resize gesture was lost past the grid's edge**, which is about a hundred
  pixels into any downward drag, leaving the panel stuck to the pointer.

Two smaller things: a `malformed` path that logged nothing, and a design pack
naming the unreachable party when it cannot know who that is.

The lesson worth keeping is that every one of these sat behind green tests. The
aggregator asserted its query had the right parts, never that a platform could
parse it. The walkthrough changed the time range and watched a scalar panel,
which has no query extractor and was immune. A browser drawing the panel is what
made them visible.

Nothing is left open on this initiative.
