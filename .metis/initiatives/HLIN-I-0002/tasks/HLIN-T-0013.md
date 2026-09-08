---
id: rendering-interface-in-hlin-view
level: task
title: "Rendering interface in hlin-view and the demo design pack"
short_code: "HLIN-T-0013"
created_at: 2026-09-07T14:04:23.116991+00:00
updated_at: 2026-09-07T15:55:47.358248+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Rendering interface in hlin-view and the demo design pack

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Give `hlin-view` the interface a design pack draws through, per the refinement to [[HLIN-A-0005]], and build `hlin-pack-demo`, the first pack. `plan()` already decides what to draw; this task is the seam between that decision and a pixel. After it, Hlin can render without the shared design system existing, and the shared design system can arrive later as another pack.

## Acceptance Criteria

## Acceptance Criteria

- [x] A `DesignPack` trait in `hlin-view` with an associated `View` type and ten required methods, no default bodies, so a pack that forgets one does not compile. A `Context` carries the treatment, the notice and the age, so adding context later does not change every signature
- [x] `hlin-view` stays free of any UI framework; the trait is generic over `View` and the crate gained no Leptos dependency
- [x] `draw(plan, pack, age) -> P::View` dispatches a plan to exactly one method, with no error path and no panic
- [x] New crate `crates/hlin-pack-demo` implementing the trait for every kind and treatment: an SVG line chart with axis labels, legend and gaps drawn as gaps; a sparkline with latest values; a stat with unit formatting and a delta; a table; a series drawn as a table; a status badge with items; a choices list; and `raw`. `Aged` shows the age, `Dimmed` and `Placeholder` show the cause, and a retired panel offers its successor
- [x] Scoped CSS shipped with the pack, with a test asserting every rule is scoped
- [x] The pack compiles for `wasm32-unknown-unknown` and for the host; server rendering is a dev-dependency feature so the browser build never carries it
- [x] 16 tests in `hlin-pack-demo` rendering to HTML, including the gap-not-a-line-to-zero check and per-state output
- [x] The existing totality test in `hlin-view` is unchanged and passes; two new tests use a recording pack to assert `draw` reaches exactly one method for every state and envelope, and that a state with no data never reaches a drawing method

## Implementation Notes

### Technical Approach
Keep the pack's components dumb: they receive data and a treatment and draw; no signals, no fetching, no knowledge of the stream. The SVG chart needs only linear scales and a path builder; resist a charting dependency. Unit formatting for the design system's named units belongs in the pack, since it is presentation.

### Dependencies
`hlin-view` and `hlin-manifest`. Independent of the shell tasks; can proceed in parallel with [[HLIN-T-0011]] and [[HLIN-T-0012]].

### Risk Considerations
Leptos version choice matters for the whole frontend; pick one here and pin it in the workspace so [[HLIN-T-0014]] inherits it rather than choosing again.

## Status Updates

**2026-09-07 — complete.** `hlin-view` gained a `pack` module holding the `DesignPack` trait, a `Context`, and `draw`. `hlin-pack-demo` is the first pack, in three modules: `chart` (SVG paths and scales), `format` (units, ages, deltas), `panels` (the components). 16 tests in the pack plus 2 in `hlin-view`; workspace at 148; `angreal check all` clean; the pack compiles for `wasm32-unknown-unknown` as well as the host.

Decisions taken during the work:

- **The trait has ten methods, not six.** Beyond one per kind it needs `series_as_table`, because `table` accepts two envelope shapes and drawing a series as a table is a different job from drawing records; `options`, because a pack must be able to draw choices and a select control will call it directly; and `skeleton` and `placeholder` for the states with no data. Every one is required, so the compiler enforces coverage rather than a review.
- **A `Context` struct rather than loose arguments.** Treatment, notice and age travel together, and adding something later will not change ten signatures.
- **Leptos 0.8 pinned in the workspace**, as the task asked, so [[HLIN-T-0014]] inherits it.
- **No charting dependency.** The chart is linear scales and a path string, about a hundred lines. A real design system will bring its own, and inheriting one here would put it in the shell's dependency graph for the life of the project to draw six demo panels.
- **Unit formatting lives in the pack.** How a byte count is written is presentation, which is what a pack owns; the vocabulary only names the unit.
- **The explanation shown to a viewer is written here from the closed set of causes**, never passed through from a platform, and a test asserts the wording appears.

**A test that taught me something about my own dispatch.** I asserted that `raw` names the envelope it is showing, for every envelope. It failed on `options.v1`, because `draw` routes `(Raw, Options)` to the pack's `options` method, which draws a readable list rather than a JSON tree. That is better output, and the guarantee `raw` actually makes is that *something sensible* appears for any envelope, not that a particular shape does. The test now says so, and a second test covers `options` being called directly, which is how a select control will use it.

**Server rendering is a dev-dependency feature.** Rendering to a string needs `leptos/ssr`, and the same code compiled for the browser must not have it. Putting it in `[dev-dependencies]` gives the test build the feature without it reaching anything that depends on the pack, which is what makes both targets compile from one source.

Note for [[HLIN-T-0014]]: `DemoPack` is a unit struct, `STYLESHEET` is the CSS to serve, and `ROOT_CLASS` is what every panel is wrapped in. `draw(plan, pack, age_seconds)` is the whole of the frontend's rendering call.
