---
id: panels-a-design-pack-draws
level: initiative
title: "Panels a design pack draws and Hlin has no word for"
short_code: "HLIN-I-0004"
created_at: 2026-09-08T00:55:00+00:00
updated_at: 2026-09-08T00:55:00+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/decompose"

exit_criteria_met: false

estimated_complexity: M
initiative_id: panels-a-design-pack-draws
---

# Panels a design pack draws and Hlin has no word for

## Problem

Hlin owns two closed vocabularies. Six view kinds say how a panel is drawn; five
envelopes say what it carries. Both are closed on purpose, and both should stay
closed: the acceptance matrix between them is what makes rendering total, and
what lets contract enforcement mean anything.

The cost is that a design system's own components are unreachable. Aurora has a
`Meter`, a `HealthPill`, a `NodeReadiness`, and a DAG `Graph`. Hlin has no word
for any of them, so a platform cannot ask for one, and `Kind::resolve` throws an
unrecognised name away on the floor:

```rust
VOCABULARY.into_iter().find(|kind| kind.name() == name).unwrap_or(Self::Raw)
```

The name is the information. Discarding it is what makes the vocabulary a
ceiling rather than a floor.

## What this is not

Not an escape hatch from the envelope vocabulary. A custom component still reads
one of the five envelopes. Opening *that* would end contract enforcement — the
shell could no longer say whether a platform's data matched what it promised —
and it is a different decision, deliberately not taken here.

## Approach

A panel keeps its `kind`, which stays a required word from Hlin's vocabulary,
and gains an optional `component`, which is a name Hlin never interprets.

```
kind: "stat"            <- Hlin's word. Always valid, always drawable.
component: "aurora.meter"  <- the pack's word. Hlin forwards it, blind.
```

Four properties fall out of that shape rather than being arranged:

1. **The fallback is guaranteed.** A pack that does not know the component draws
   the declared kind, which validation already proved drawable against the
   declared envelope. There is no new way for a panel to be undrawable, so
   rendering stays total.
2. **The contract does not move.** `component` is presentational, like `kind`.
   The acceptance matrix, the diff rules and the debounce machinery are
   untouched, and a platform adding one is not a contract change.
3. **Hlin stays ignorant.** It forwards a string. `grep -ril aurora crates/`
   must still return nothing.
4. **Degradation is demonstrable.** The same panel, the same manifest, the same
   data, drawn as a DAG under one pack and a table under another — which the
   gallery front end can switch between without a rebuild.

## Success Criteria

- [ ] A platform can name a component Hlin has never heard of and get it drawn
- [ ] A pack that does not know that component draws the declared kind instead,
      with nothing on screen suggesting a failure
- [ ] Hlin contains no reference to any pack's component names
- [ ] `DesignPack` says explicitly what a pack does about components it does not
      know; no default implementation hides the decision
- [ ] The demo shows a real Aurora component that Hlin has no vocabulary for,
      beside the same panel drawn by a pack without it
- [ ] A curated surface exists in the demo, composed from both platforms, so
      opening it shows composition rather than an empty grid

## Status Updates

### 2026-09-08 — a graph Hlin cannot describe, drawn by a pack Hlin cannot name

`Panel` gained an optional `component`. `Kind::resolve` still owns the closed
vocabulary and still falls back to `raw`; nothing about that changed. What
changed is that a name outside the vocabulary now reaches the pack instead of
being dropped.

**The seam, end to end.** `hlin-sample-platform` declares `pipeline` as
`kind: "table"`, `envelope: "records.v1"`, `component: "aurora.graph"`. The
shell forwards the string through `CatalogPanel` without reading it.
`RenderPlan::drawn_by` carries it to `draw`, which offers it to the pack before
the vocabulary. Aurora reads the same rows as nodes and edges and draws a DAG.
The demo pack returns `None` and the panel draws the table it also is.

**The boundary held.** `grep -ril aurora crates/` returns two files, both in
`hlin-sample-platform` — a *platform*, naming a component, which is the intended
use. The shell, the vocabulary, the wire types and the composition machinery
contain no design system's name.

**One decision worth recording.** A viewer's kind override outranks the
platform's component. Switching a panel to a table and getting a graph anyway
would make the kind picker a lie, so `component` is only forwarded when
`kind_override` is absent.

**What is still closed, deliberately.** Envelopes. `aurora.graph` reads
`records.v1` — a platform and a pack agreeing that `id`, `label`, `state` and
`depends_on` describe a graph. Hlin still believes it is a table, the acceptance
matrix is unchanged, and contract enforcement still means what it meant. Opening
the envelope vocabulary would end that, and is not this.

**Tests.** Four in `hlin-view` pin the behaviour, including a walk of the whole
state × kind × envelope matrix with an unrecognised component named, asserting
every combination still draws exactly once. Two browser tests assert both halves
against a real browser: drawn under Aurora, fallen back under the demo pack with
nothing on screen suggesting a failure.

**The curated surface**, the other half of the ask: `angreal demo compose`
builds eight panels from two platforms across every view kind, two of them
asking for a component. Screenshots 93 and 94 are the same layout under both
packs.

209 Rust tests, 18 browser tests, checks and walkthrough clean.

### Known flake

One full browser run in three failed in `surface.spec.js` — the resize test,
timing out waiting for `section.panel` to exist at all, after the drag test
before it had passed. It did not reproduce running the file alone or on two
subsequent full runs, and it is a hang rather than a slow refetch. The suite
sets `retries: 0` on the grounds that a flake here is a real signal about the
stream, so this should be chased rather than accepted: the likely mechanism is
the recorded cost that every layout write drops the surface, and a surface
rebuilt at the moment the next test subscribes.
