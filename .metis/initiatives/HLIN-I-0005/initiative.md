---
id: components-that-talk-back
level: initiative
title: "Components that talk back"
short_code: "HLIN-I-0005"
created_at: 2026-09-08T01:40:00+00:00
updated_at: 2026-09-08T01:40:00+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/decompose"

exit_criteria_met: false

estimated_complexity: M
initiative_id: components-that-talk-back
---

# Components that talk back

## Problem

Everything between the shell and a design pack flowed one way. A pack was handed
a treatment, a notice, an age and an envelope, and drew them. A component could
not ask for anything, so every control a person could touch was Hlin's own
chrome, and a design system could style the panels but never the interaction.

Two halves were missing, and only one of them was obvious.

**Out.** A component had no way to say what a person did. A clicked node, a
brushed range, a chosen facet: all of it stopped at the pack.

**In.** Less obvious and older. A platform declaring a `select` may name an
endpoint listing what it will accept, and `PanelControl.options` carried that
URL to the browser under a comment reading *"Not yet used: reaching it means the
shell fetching on the viewer's behalf, because the browser cannot call a
platform directly."* Nobody ever did. So the chrome's filter was a free-text
box, and a person filtering by cluster had to know the value was spelled
`orebank-eu-west` and type it exactly. A component drawing its own filter would
have had nothing better to offer.

## Approach

**A closed vocabulary again, for the same reason.** An `Intent` is one of a
small set of things, and every one maps onto something the shell already does
and already has tested: `Select` writes where the panel's own control writes,
`Range` asks what the time picker asks. A general callback would have been less
code and would have moved the problem — the shell would then have to interpret
whatever arrived, which is not something it can be tested against or degrade
from. Nothing here gives a design system a capability the chrome does not have,
which is the property that makes forwarding it safe.

**What is deliberately absent** is anything a component can do alone. A node
that expands, a column that sorts, a tooltip: local to the component, the pack's
own state, and a round trip for nothing.

**An emitter that goes nowhere is not a degraded mode.** It is what a pack gets
outside a live surface, so a component wires its handlers once rather than
having two versions of itself.

**Options are a lookup, not a stream.** They do not depend on the time range or
on a selection, so they are a route rather than a change to the aggregator —
which keeps generations, dedup, settle and backoff untouched.

**The route takes a panel, not a URL.** `/api/options?url=...` would make the
shell an open proxy for its own credential. The caller names a platform, a panel
and a control; the shell looks all three up in the manifest it already holds,
and the only address fetched is one that platform declared about itself.

## Success Criteria

- [ ] A component can emit a filter change and the shell asks the platform a
      different question because of it
- [ ] The choice survives a reload, because it was stored rather than drawn
- [ ] The chrome's own control and a component's filter read one source, so they
      cannot disagree about what is set
- [ ] The shell will not fetch an address it was handed
- [ ] A platform that lists nothing still filters, by typed value, as before
- [ ] Rendering stays total: a plan nobody wired still draws

## Status Updates

### 2026-09-08 — the round trip, in one component

`hlin-view::intent` holds `Intent`, `Emit` and `Control`. `RenderPlan` gained
`.interactive(controls, emit)` beside `.drawn_by(component)`, so `plan`'s three
arguments and its twenty-five call sites are unchanged, and `Context` carries
both to the pack.

`/api/options/{platform}/{panel}/{control}` is the in half. The four cases were
exercised against the running demo: a control with options answers the list, one
without answers an empty list, an unknown panel and an unknown platform are both
refused with 404. The refusals are the point — they are what stop it being a
proxy.

Aurora's `aurora.faceted` is the round trip in a single component: it reads
`context.controls` for what the platform accepts and what is chosen, draws
pills, and sends `Intent::Select` on a click. Four browser tests assert the
whole loop rather than its parts — a click changes the drawn SVG, exactly one
pill shows as set, and the choice survives a reload.

**`Context` is no longer `Copy`.** It holds an `Arc`, so `draw` clones it before
offering a component that may decline. `RenderPlan` lost `PartialEq` for the
same reason; nothing compared whole plans.

### Correction worth recording

`the_reference_manifest_is_valid` asserted a hardcoded panel count and had been
failing since the live-data commit. It went unnoticed because the command used
to total results mis-parsed cargo's `FAILED.` line, and because a failing test
binary stops the workspace run — so the runs reported as "209 passing, 0 failed"
were both miscounted and truncated. The true figure with it fixed is 281.

The lesson is the one this repository keeps relearning: a test asserting a tally
of panels asserts nothing true and breaks whenever the demo grows. That one now
counts from the document, as the two browser tests fixed for the same reason do.
