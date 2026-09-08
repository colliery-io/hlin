---
id: 001-panels-declare-explicit-envelopes
level: adr
title: "Panels declare explicit envelopes, decoupled from view kinds"
number: 1
short_code: "HLIN-A-0003"
created_at: 2026-08-04T14:24:44.108694+00:00
updated_at: 2026-08-04T14:26:38.924344+00:00
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

# ADR-3: Panels declare explicit envelopes, decoupled from view kinds

## Context

A panel declaration names a view kind and a data endpoint, and the endpoint returns a typed envelope. The design question is whether the envelope type is implied by the kind (one envelope per kind, no indirection) or named explicitly in the panel declaration as its own field.

The vision ([[HLIN-V-0001]]) constrains this from two directions: the shell renders everything from a bounded vocabulary, and the data envelope must be designed so the future declarative view spec "drops in without reshaping anything."

## Decision

Panel declarations name both a `kind` and an `envelope`, as separate fields drawn from two separately governed vocabularies. Envelope names are versioned (e.g. `series.v1`); each view kind declares which envelopes it accepts; the shell validates the kind/envelope pairing at manifest ingestion, and an invalid pairing makes the panel unavailable (malformed), never a shell error.

### Refinements (2026-09-07, after design review)

- **`kind` is a default, not contract.** Because users may switch a panel between any kinds that accept its envelope, the platform's declared `kind` is the default rendering, not something consumers pin to. The data contract of a panel is `envelope` + `data` (with `key`, `params`, and `lifecycle`); changing `kind` is a non-contract change and never breaking. The manifest specification's contract-content and diff tables reflect this.
- **Envelope versions are permanent.** Platforms can never be forced to upgrade, so once any platform names `series.v1` the shell must accept it forever. The envelope vocabulary only grows; a version suffix signals that a new, incompatible shape exists alongside the old one, never that the old one is going away. Retirement of an envelope version is not a supported operation.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Envelope implied by kind | One vocabulary, no indirection, nothing to validate | Data shape and rendering evolve in lockstep: a new envelope version forces a new kind name; envelope reuse across kinds impossible; view-spec escape hatch requires reshaping the contract later | Medium | Low |
| Explicit envelope field (chosen) | Data contract and rendering vocabulary evolve independently; envelopes reusable across kinds; view spec drops in as a panel that names an envelope plus a spec tree instead of a kind | Two governed vocabularies plus a compatibility mapping between them | Low | Medium |

## Rationale

Kinds and envelopes change for different reasons on different cadences. A kind changes when the design system decides something should *look* different; an envelope changes when platforms need to *say* something different. Coupling them means every data-shape evolution is also a rendering-vocabulary event, which puts the design system in the path of platform data needs — exactly the bottleneck shape the vision forbids.

Decoupling also makes concrete two things the vision promises: several kinds rendering the same envelope (a `series.v1` drawn as `timeseries` or as `sparkline`), and the declarative view spec arriving later as "envelope + spec tree" without touching the envelope contract — the drop-in property the vision requires the envelope design to preserve.

## Consequences

### Positive
- The envelope vocabulary can grow at platform speed while the kind vocabulary stays deliberately slow, each with its own admission bar.
- Envelope reuse across kinds falls out for free, and users could plausibly switch a panel between kinds that accept the same envelope — a composition-level customization requiring no one to deploy anything.
- The view-spec escape hatch has a defined seam and requires no contract reshaping when its day comes.

### Negative
- The kind→accepted-envelopes mapping is a new governed artifact; it must live with the view registry and be versioned with it.
- Manifest validation gains a step (pairing check), and platform authors have one more field to get right — though the failure mode is a well-classified unavailable panel, not breakage.

### Neutral
- The envelope vocabulary's owner (design system vs. a contracts group) is not settled by this ADR; it lands with the view registry location decision.
