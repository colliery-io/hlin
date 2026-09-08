---
id: hlin
level: vision
title: "hlin"
short_code: "HLIN-V-0001"
created_at: 2026-08-04T05:02:44.175681+00:00
updated_at: 2026-08-04T14:08:51.939819+00:00
archived: false

tags:
  - "#vision"
  - "#phase/published"


exit_criteria_met: false
initiative_id: NULL
---

# Hlin Vision

**One vantage over many systems, assembled by the people who use them.**

Hlin is named for the Norse goddess who watches over those she is named to protect.

## Purpose

We are building roughly a dozen Rust platforms. Each has a Leptos frontend served from its own root, each ships on its own cadence, and each is developed by a team that should not have to coordinate with the other eleven to release.

The cost lands on the people using them. Operational context is spread across a dozen origins, so answering a question that spans two systems means holding two tabs and doing the correlation by hand. There is no place where "how are things" can be answered. Each frontend is coherent on its own; together they are twelve tools rather than one product.

The obvious fixes are both wrong. Building a thirteenth hand-written dashboard puts every cross-cutting view behind a central team's backlog and goes stale the moment a platform changes. Composing the frontends at runtime means a dozen WASM binaries in one document, each with its own allocator and reactive runtime, which is not viable at this count and would not be viable at half it.

## Product/Solution Overview

Hlin is a composition shell.

Each platform declares, at runtime, what it can show. Hlin discovers those declarations, renders every declared panel through a single design system, and lets a person select panels from any platform and arrange them into a surface they authored.

Nothing crosses the boundary except data and a declared view kind. Hlin does the rendering; platforms do not ship code into it.

That single constraint produces most of the properties we want. Design consistency is structural rather than enforced by review, because every panel is drawn by the same component library. Drag-and-drop composition is tractable, because every panel is the same kind of object to the layout engine. Adding a platform costs the shell nothing, because the shell was never compiled against it.

### What Hlin is not

**Not a reverse proxy.** Path-prefixed routing and single-origin sessions sit underneath Hlin and make cross-platform data fetching and auth tractable. That plumbing is necessary and it is not the product.

**Not micro-frontends.** No runtime composition of independently built WASM modules; not now, not as a later phase. Panels are data, not applications.

**Not a query layer.** Hlin shows what platforms choose to expose. It does not reach into anyone's database, and it defines no query language over platform internals.

**Not a general dashboarding tool.** Grafana exists and is better at being Grafana. Hlin composes views authored by the teams that own the systems, in the vocabulary of those systems.

**Not a central bottleneck.** Shipping a panel does not require a Hlin release, a Hlin PR, or a design review.

## Current State

A dozen independent Rust platforms, each with its own Leptos frontend at its own origin, each on its own release cadence. Cross-system questions are answered by hand across browser tabs. No shared vantage exists, and no mechanism exists for one to emerge without central coordination.

## Future State

One shell where every platform's declared panels render through a single design system, and where the people using the systems — not a central team — compose the cross-platform views they need. Adding a platform, a panel, or a navigation entry is a deploy of that platform alone; the shell is never rebuilt to accommodate a child.

## Major Features

- **The manifest.** Each platform serves a document describing itself at a well-known path: navigation entries, offered panels, the parameters each panel responds to, health and summary endpoints, and lifecycle status for anything deprecated. Three rules keep it survivable across a dozen independent release cadences: the schema version is monotonic and additive with unknown fields ignored; icons and view kinds are names from a shared vocabulary rather than code; a malformed or absent manifest degrades to a plain link with a warning indicator, never a shell error.
- **Discovery.** Hlin reads a platform list from runtime configuration and polls each manifest. The registry sits behind a trait, so a push-based model with heartbeat expiry, or orchestrator-native label discovery, can replace configuration later without reshaping anything above it.
- **Panels.** A panel declaration names a view kind and a data endpoint returning a typed envelope for that kind. Shell-level controls, notably time range, drive every panel that declares the corresponding parameter. Fan-out is aggregated shell-side, deduplicated across panels requesting the same data, and delivered to the browser over a single stream.
- **Views.** The view registry lives in a companion crate beside the design system, so the component library stays free of Hlin's vocabulary. Adding a view kind is a release of that crate; platforms adopt it on their own schedule by naming it, and never trigger a shell rebuild themselves.
- **Customization.** Composition-level customization comes first: users choose panels, arrange them, set titles and thresholds and time ranges. A bounded declarative view spec is the deliberate escape hatch for genuinely bespoke panels, and it is not built until composition-level customization has demonstrably failed a real case. Embedding a platform's own frontend in a panel is not a supported mechanism.

## Success Criteria

A team ships a new panel on Tuesday morning. Someone on another team has it on their dashboard Tuesday afternoon, next to panels from two other platforms, with no Hlin release, no design review, and no conversation between the two teams.

The bet: that the set of valuable cross-platform views is larger than any central team can enumerate, and that the constraint of a shared rendering vocabulary is a smaller cost than the coordination it removes. The second half is the part that could be wrong. If platform teams find the vocabulary too narrow to express what their systems actually need to show, they will route around it, and the shell becomes a link farm with extra steps. Guarding against that is what vocabulary governance is for, and it is the thing to watch in the first two adoptions.

## Principles

**The shell renders everything.** Panels cross the boundary as data plus a view kind drawn from a shared vocabulary. A platform names `timeseries`; Hlin decides what a timeseries looks like. Consistency follows from the architecture instead of from discipline.

**Platforms are autonomous.** Adding a platform, a panel, or a navigation entry is a deploy of that platform. Discovery happens at runtime against a manifest each platform serves about itself. Hlin is never rebuilt to accommodate a child.

**The manifest is a public API.** Panels are a declared contract, not an implementation detail. They are versioned, diffed in CI, and deprecated with a window and a named successor. Removing a panel key without a major version is a breaking change; the shell detects it at runtime and flags it, and no build anywhere has to fail for the contract to be enforced.

**Vocabulary is additive and governed.** View kinds are a bounded set owned by the design system. New kinds are added deliberately, with an owner and a bar for admission. Unknown kinds degrade to a fallback; they never break a layout.

**Composition belongs to users.** Selecting and arranging panels requires a deploy from nobody. The set of useful cross-platform views is not knowable in advance by any central team, so we do not try to enumerate it.

**Rendering is total over its inputs.** Every panel is in exactly one state at all times: loading, ready, stale, or unavailable, with unavailability distinguishing unreachable from malformed from unknown from deprecated. There is no state a platform can put a panel into that the shell does not have a rendering for.

## Constraints

- Panels cross the platform/shell boundary as data plus a declared view kind only; no code ships across it.
- No runtime composition of independently built WASM modules, in any phase.
- The shell must never be rebuilt or released to accommodate a platform change.
- Unknown view kinds and malformed manifests degrade gracefully; they never break a layout or produce a shell error.

## Design Questions, Answered

All five are now decided. Each names the decision that closed it.

- **The manifest schema.** A document at a well-known path, with contract identity separated from presentation, so a fingerprint moves only when a promise moves. Specification HLIN-S-0001.
- **The data envelope per view kind, and the initial vocabulary.** Five envelopes and six kinds, named separately and paired by an acceptance matrix, so data shape and rendering evolve on their own cadences. Specification HLIN-S-0002, decision HLIN-A-0003.
- **Where the view registry lives.** A companion crate, `hlin-view`, so the design system stays a plain component library that platform frontends can use without taking on Hlin's contract types. Decision HLIN-A-0005.
- **Layout persistence and sharing.** One owner per layout, personal or published to a shell-wide gallery, shared read-only with fork to edit. A layout referencing a platform the viewer cannot reach renders normally, with the refused panels showing as forbidden. Decision HLIN-A-0007.
- **Authorization.** Authentication is hoisted to Hlin, the shell forwards a signed identity on every request, and platforms decide at fetch time. The shell interprets nobody's policy. Decision HLIN-A-0004, specification HLIN-S-0004.

What remains open is recorded in those specifications rather than here.

## Crates

```
hlin              shell binary
hlin-manifest     contract crate: schema, panel entries, lifecycle, envelopes
hlin-view         view registry: kinds, the kind/envelope acceptance matrix, rendering
```

## Amendments

**2026-09-07.** Two sentences were corrected once the top-down design initiative ([[HLIN-I-0001]]) reached decisions that contradicted them.

- *Enforcement.* The principle "The manifest is a public API" said a breaking change "fails the build that attempts it". There is no build to fail: enforcement is runtime, in the shell, which is what keeps a dozen independently released platforms free of a shared gate ([[HLIN-A-0002]]).
- *The view registry.* Architecture said the registry lives in the design system. It lives in a companion crate beside it, so the twelve platform frontends that use the design system for their own purposes never take on Hlin's contract types ([[HLIN-A-0005]]).

The "Open design questions" section became "Design Questions, Answered" in the same pass; all five now have decisions.
