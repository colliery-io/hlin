---
id: 001-the-view-registry-lives-in-a
level: adr
title: "The view registry lives in a companion crate, hlin-view"
number: 1
short_code: "HLIN-A-0005"
created_at: 2026-09-07T11:59:38.014232+00:00
updated_at: 2026-09-07T12:00:43.024966+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-5: The view registry lives in a companion crate, hlin-view

## Context

The view registry is the thing that knows which kinds exist, which envelopes each accepts, and how each kind renders every panel state ([[HLIN-S-0002]]). The vision left its home open: inside the design system, or in a companion crate that depends on it. The choice determines whether `hlin-view` exists, who owns the vocabulary, and what a platform pulls in when it depends on the design system for its own frontend.

The design review also corrected an overstatement: vocabulary growth does rebuild the shell, whoever owns the registry. The question is therefore not "who keeps the shell out of the loop" but "who is the right owner, and what is the smallest dependency surface."

## Decision

The view registry lives in a companion crate, `hlin-view`, which depends on the design system and on `hlin-manifest`. It owns the kind vocabulary, the kind→envelope acceptance matrix, the total rendering function from (state, envelope, kind) to a design-system component, and the `raw` fallback.

The design system remains a pure component library with no Hlin vocabulary in it. It knows how to draw a line chart; `hlin-view` knows that `timeseries` accepts `series.v1` and draws it with that chart.

Adding a kind is an `hlin-view` release followed by a shell redeploy. The design system and `hlin-view` are version-paired: each `hlin-view` release pins the design-system version it was built against.

### Refinement (2026-09-07, from [[HLIN-I-0002]])

The shape holds; the direction of the drawing dependency is corrected. `hlin-view` does not depend on the shared design system. It **defines the rendering interface**: for each kind and each panel state, what a drawing layer must provide. A *design pack* implements that interface, and the shell binary picks one at build time.

The shared design system ships a pack. The demo ships a minimal pack of its own, with hand-rolled SVG charts. A platform with a reason to look different could ship a third. None of them are depended on by `hlin-view`, so its totality test stays a pure property of the vocabulary, and Hlin can render before the shared library is published.

What does not change: `hlin-view` owns the kinds, the acceptance matrix, the state machine and the `raw` fallback, and the design system still carries no Hlin vocabulary. Version pairing becomes the pack's obligation rather than `hlin-view`'s: a pack pins the `hlin-view` interface version it implements.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Companion crate `hlin-view` (chosen) | Design system stays Hlin-agnostic; platforms using the design system for their own frontends pull in no Hlin types; vocabulary ownership is explicit and separable from component ownership | One more crate; version pairing to maintain | Low | Low |
| Inside the design system | One fewer crate; the vision's default assumption | Every consumer of the design system, including all twelve platform frontends, transitively depends on `hlin-manifest`; the component library's release cadence becomes the vocabulary's cadence | Medium | Low |
| Inside the shell binary | Simplest today | The shell team owns the vocabulary, which the governance principle argues against; nothing outside the shell can test a kind's rendering | Medium | Low |

## Rationale

The design system is shared by a dozen platform frontends that have nothing to do with Hlin. Putting Hlin's vocabulary inside it makes every one of them carry the manifest contract as a transitive dependency and ties vocabulary admission to the component library's release process. A companion crate keeps the two concerns separable: the design system answers "what can be drawn," `hlin-view` answers "what Hlin calls it and what data it takes."

It also makes the vocabulary independently testable. `hlin-view` can hold the totality tests (every kind renders every state, every envelope has at least `raw`) without a running shell.

## Consequences

### Positive
- The crate list is settled: `hlin` (shell binary), `hlin-manifest` (contract types, envelopes, canonicalization, hashing), `hlin-view` (registry and rendering).
- Totality of rendering is a property of one crate and can be enforced by a test in that crate.
- Platforms depending on the design system are untouched by any Hlin change.

### Negative
- Version pairing between `hlin-view` and the design system is a real maintenance obligation; a mismatch is a compile error in the shell, which is the acceptable failure mode.
- Two crates now define halves of the same contract (`hlin-manifest` the envelopes, `hlin-view` their renderings); the acceptance matrix is the seam and must be kept in `hlin-view` only.

### Neutral
- The vocabulary specification ([[HLIN-S-0002]]) was written to be indifferent to this decision and needs no change; its "owner" column resolves to `hlin-view`.
