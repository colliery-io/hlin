---
id: a-real-design-system-unmodified
level: initiative
title: "A Real Design System, Unmodified"
short_code: "HLIN-I-0003"
created_at: 2026-09-07T21:40:00.000000+00:00
updated_at: 2026-09-07T22:05:00.000000+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/discovery"


exit_criteria_met: false
estimated_complexity: M
initiative_id: a-real-design-system-unmodified
---

# A Real Design System, Unmodified Initiative

## Context

[[HLIN-I-0002]] proved the shell runs and that a person can compose a surface in
a browser. Everything it drew went through `hlin-pack-demo`, which exists to
prove the rendering interface and is explicitly disposable. Nothing has
established that the seam is in the right place, because the only thing on the
far side of it was written by the same hands, in the same repository, at the
same time.

Colliery's Aurora Dark is a published design system with its own release
cadence, already the basis of other control-plane apps. It is the honest test.

The goal is a demo composing a front end through Hlin and rendered by Aurora
Dark, with Hlin containing no knowledge that Aurora exists.

## Goals & Non-Goals

### The decision this initiative rests on

**Design packs implement Hlin's types.** Aurora gains a `hlin` cargo feature.
Off by default, it adds `hlin-view` as an optional dependency and an
implementation of `DesignPack`. On, it makes Aurora a design pack.

This was chosen over the alternatives after they were tried and found to be half
measures. A binding crate inside Hlin makes Hlin depend on one design system,
which is the coordination point the architecture exists to remove. A binding
crate in a third repository works but adds a thing to version whose only job is
to introduce two crates to each other. Configuration alone cannot do it: a
mapping table can choose *which* component draws an envelope, but something must
still turn a `Series` into an SVG path and a `Status` into a colour and a label,
and that adaptation is most of the work.

Putting the implementation in the design system is the arrangement where each
side owns what it knows. Hlin owns the vocabulary and the interface. A design
system owns how its own components are driven. Neither is rebuilt for the other.

### What "unmodified" now means

Aurora is edited once, additively, behind a feature that is off by default.
Every existing consumer of `colliery-io-aurora` is unaffected: no new
dependency, no new compile cost, no behaviour change. That is the measure, and
it is checkable.

**The exit criteria:**

1. Aurora's default build is byte-for-byte unaffected by the `hlin` feature.
2. A grep for "aurora" across Hlin's `crates/` returns nothing.
3. `angreal demo up` renders every panel through Aurora, and the browser suite
   from [[HLIN-T-0017]] passes against it.

**Goals:**
- `hlin-ui` generic over the pack rather than compiled against a named one.
- The stylesheet part of the pack contract rather than a convention.
- A custom-property contract for the shell's own chrome, so a dark pack does not
  sit inside a light shell.
- `hlin-manifest` and `hlin-view` published, since a published design system
  cannot depend on crates that only exist on one laptop.
- Aurora's `hlin` feature, implemented in Aurora.
- A demo front end that picks Aurora, and the browser suite passing on it.

**Non-Goals:**
- Retiring `hlin-pack-demo`. It stays as the reference implementation of the
  interface and as what this repository can demonstrate itself with, depending
  on nothing published.
- Forwarding arbitrary pack components. Decided: a separate initiative. It
  changes what design consistency means and needs versioning discipline of its
  own.
- A second pack, runtime pack selection, or theming.

## Architecture

### What is already right

`hlin-view` defines `DesignPack` and depends on nothing concrete: no Leptos, no
design system, only the contract crate, serde and thiserror. Every method is
required with no defaults, so a pack cannot forget a kind and cannot weaken
totality. A third party writing a bad pack cannot break Hlin's guarantee. None
of that changes.

### What is wrong, and all of it is in Hlin

| Where | Problem |
|---|---|
| `hlin-ui/src/app.rs` | Imports and instantiates `DemoPack` by name |
| `hlin/src/server.rs` | Serves `hlin_pack_demo::STYLESHEET` at `/assets/pack.css` |
| `hlin-ui/app.css` | A hardcoded light palette for the bar, picker and grid |

Each is a place where changing the design system means editing Hlin.

### Two additions to the pack contract

**The stylesheet moves onto the trait.** What a pack needs to look right is part
of what a pack is, and a pack that implements only `DesignPack` currently has no
way to get its CSS onto the page.

**The shell declares custom properties for its own chrome**, and the pack fills
them. Hlin never learns Aurora's token names; Aurora never learns what a picker
is. Same shape as the rest of the architecture.

### Publishing, which this initiative forces

A crate published on crates.io cannot depend on a path. If Aurora is to declare
`hlin-view` under a feature, `hlin-view` and `hlin-manifest` must be published.
Two consequences:

- **Naming.** Colliery's convention is org-prefixed, as `colliery-io-aurora`
  already is. These would be `colliery-io-hlin-view` and
  `colliery-io-hlin-manifest`, imported as `hlin_view` and `hlin_manifest`.
- **Weight.** `hlin-manifest` pulls `sha2`, `semver` and `serde_jcs` for the
  contract fingerprint, none of which a pack uses. They should be feature-gated
  so a design system taking this dependency gets the envelope types and nothing
  else.

Publishing also means the interface is now a public API with the versioning
obligations that implies, which is the real cost of this decision and worth
naming plainly.

**Nothing is published in this initiative.** Decided: get ready, stay local.
Aurora is vendored into this repository and depended on by path, so the demo
builds from a clean clone with no registry involved and the ergonomics can be
felt before anything is claimed on a namespace nobody can take back. The
publication work here is the preparation: trimming what a pack has to pull in,
settling names, and making sure the interface is one somebody would want to
implement. Releasing is a later decision made with evidence.

The vendored copy is a staging area, not a fork. The `hlin` feature written
there has to be contributed back to `aurora-dark` before either crate is
published, and the vendor directory says so.

### The Leptos coupling, stated

A pack is compiled into the frontend binary, so its Leptos and the frontend's
must resolve to one version. Caret ranges suffice; hard pins are not needed.
Packs and the frontend cross a Leptos major together and are independent in
between. This coupling never reaches platform teams, who send data and a name.

### Charts

Aurora has no line chart, so `timeseries` and `sparkline` have no equivalent and
are drawn by hand in SVG using Aurora's tokens. That is real implementation and
the honest cost of a general design system without a plotting primitive. Every
future pack pays it again. Recorded as a finding about the seam rather than
hidden: if a third pack pays it too, a shared chart primitive has earned its
place somewhere.

## Detailed Design

Six slices, in dependency order. The first four are Hlin and must land before
Aurora can implement against them.

1. **Generic frontend.** `hlin-ui` becomes generic over `P: DesignPack` with
   `P::View: IntoView`. The binary that mounts it chooses. `hlin-pack-demo`
   moves from an import in the library to a choice in the demo binary.
2. **Stylesheet in the contract.** A trait method returning what the pack needs
   served. The shell serves whatever it is handed at `/assets/pack.css`.
3. **Chrome tokens.** Hlin declares the custom properties its bar, picker and
   grid use, with defaults that work standalone. A pack maps its own tokens onto
   them.
4. **Ready to publish, not published.** Feature-gate the fingerprinting
   dependencies in `hlin-manifest` so a pack pulls the envelope types and
   nothing else, settle the published names, and make the two crates releasable.
   No release.
5. **Aurora vendored, with a `hlin` feature.** `aurora-leptos` copied into
   `vendor/`, depended on by path, and given an optional `hlin-view` dependency
   plus a gated module implementing `DesignPack` with Aurora's components. The
   stylesheet method returns `AURORA_CSS` and the chrome mapping. The two chart
   kinds are hand-drawn SVG using Aurora's tokens.
6. **The demo on Aurora.** A frontend binary under `examples/` that mounts
   `hlin-ui` with Aurora's pack, the browser suite's selectors updated to
   Aurora's markup, screenshots looked at, and the exit criteria checked.

## Decisions Taken

**Where the Aurora demo binary lives.** An `examples/` directory in this
repository, outside the workspace members. `crates/` stays free of Aurora, which
is the exit criterion, and the demo still runs from a fresh clone, which a demo
in a third repository would not. Examples are allowed to name concrete things;
that is what they are for.

## Alternatives Considered

- **A binding crate inside Hlin.** Rejected: makes Hlin depend on one design
  system, which is the coordination point the architecture removes.
- **A binding crate in a third repository.** Workable, rejected as a half
  measure: a whole release cadence whose only purpose is introducing two crates.
- **Configuration mapping publisher types to components.** Rejected as
  insufficient alone. Configuration can select which component draws an
  envelope; it cannot adapt one shape into another, and adaptation is the work.
  It converges on component forwarding, which is deferred.
- **Runtime-loaded packs.** Rejected with micro-frontends, at any phase.

## Implementation Plan

Decomposed 2026-09-07 into six tasks, one per slice:

| Task | Slice | Depends on |
|---|---|---|
| [[HLIN-T-0019]] | `hlin-ui` generic over its pack | nothing |
| [[HLIN-T-0020]] | The stylesheet on the trait | T-0019 |
| [[HLIN-T-0021]] | Chrome custom properties | T-0020 |
| [[HLIN-T-0022]] | Contract crates ready to publish | T-0019, T-0020 |
| [[HLIN-T-0023]] | Aurora vendored, with its `hlin` feature | T-0019 to T-0022 |
| [[HLIN-T-0024]] | The demo on Aurora, and the three measures | everything |

Strictly sequential. Each of the first four changes the interface Aurora will
implement, so writing the pack before they land means writing it twice.

Exit: the three measures in [[HLIN-T-0024]] pass, and there is a written record
of what was awkward about implementing the pack, which is the finding this
initiative exists to produce.

## Status Updates

### 2026-09-07, later — complete, all three measures hold

Six tasks done. Aurora Dark draws Hlin's panels, Hlin does not know Aurora
exists, and Aurora's default build does not know Hlin exists.

| Measure | Result |
|---|---|
| Aurora's default build unaffected | `js-sys`, `leptos`, `web-sys` with the feature off; `hlin-view` and `serde_json` added only with it on |
| `grep -ril aurora crates/` | nothing |
| Demo and browser suite on the new pack | 15 tests pass against Aurora, and the same 15 against the demo pack |

The seam held. Three defects were found on the way, all of them the sort only an
outside implementor hits:

- **`hlin-view` did not re-export the envelope types**, so implementing
  `DesignPack` needed a second dependency for types in the trait's own method
  signatures.
- **A layout write could be undone by an older write's answer**, because saving
  is fire-and-forget and the shell's reply replaces the whole draft. Found as a
  one-in-fifteen browser flake and fixed with a write ticket.
- **The stylesheet was not part of the contract**, so a pack that implemented
  every method still had no way to get its CSS onto the page.

Two things are true now that were not before. The shell binary depends on no
design pack at all, and neither does `hlin-ui`; a front end is a binary that
picks a pack and mounts the library, and there are two of them differing by one
identifier. And the browser suite asserts nothing about any pack's markup, so it
runs against either.

**What is left, and it is deliberate.** Nothing is published. The vendored
Aurora is a staging area whose `hlin` feature has to be contributed upstream
before release, and `vendor/README.md` says so. Two decisions are recorded for a
person in [[HLIN-T-0022]]: whether the crates take the `colliery-io-` prefix
when `hlin` is not a generic name, and what version to publish at once the
interface has a consumer.

### 2026-09-07 — designed, pending one placement decision

Opened in discovery. The blocking decision is made: design packs implement
Hlin's types, and Aurora gets a `hlin` feature that is off by default.

Three alternatives were considered and rejected in the course of getting here,
and two of them were mine. A binding crate inside Hlin was started and abandoned
once it was clear it made Hlin own a design-system dependency. A third
repository was proposed and is a half measure. Configuration alone was raised
and is insufficient, though the reasoning behind it converges on component
forwarding and is worth revisiting when that initiative opens.

The decision has a cost that was not obvious at the start and is now the largest
piece of work in the initiative: a published design system cannot depend on
unpublished crates, so `hlin-manifest` and `hlin-view` have to be released, and
the pack interface becomes a public API with the versioning obligations that
brings.
