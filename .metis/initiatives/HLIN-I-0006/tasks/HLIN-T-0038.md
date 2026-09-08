---
id: exercise-intent-range-in-a-browser
level: task
title: "Exercise Intent::Range in a browser"
short_code: "HLIN-T-0038"
created_at: 2026-09-08T01:56:21.660789+00:00
updated_at: 2026-09-08T04:32:10.634172+00:00
parent: HLIN-I-0006
blocked_by: []
archived: false

tags:
  - "#task"
  - "#tech-debt"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0006
---

# Exercise Intent::Range in a browser

## Objective

`Intent::Range` is plumbed end to end — `hlin-view/src/intent.rs`,
`hlin-ui/src/app.rs` `span()`, and the emit handler that updates the picker — and
no pack emits it, so no browser test exercises the handler. The unit test covers
the emitter, not the path.

Finding 13 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (test coverage)

### Priority
- [x] P3 - Low

### Technical Debt Impact
- **Current Problems**: Plumbing that could be mistaken for a feature.
- **Benefits of Fixing**: Either a brushing component exists, or the path is
  proven by a test that emits the intent directly.
- **Risk Assessment**: Low; nothing depends on it yet, which is the point.

## Acceptance Criteria

- [x] A pack component that emits `Intent::Range` — a chart brush in
      `vendor/aurora-leptos/src/hlin.rs` is the natural one — or a test hook that
      does
- [x] Browser test: the emitted range changes the picker's state and every
      panel's query

## Implementation Notes

### Technical Approach
`plot()` in `hlin.rs` already computes x positions; a brush maps a pointer drag
back through the same scale.

### Dependencies
None.

## Status Updates

### 2026-09-08 — a brush, and the defect it exposed

`aurora.brush` draws the same chart with a drag handler over it, and the sample
platform declares `throughput-brush` asking for it. Dragging across it emits
`Intent::Range`.

**This is the other direction of the round trip.** `aurora.faceted` sends
`Intent::Select`, which changes one panel; this changes the *surface*, so every
panel beside it moves to the window that was dragged. That is the path that had
no component emitting it and therefore no browser ever exercising the handler.

The mapping is the element's own bounding box rather than the SVG's coordinate
system, because the chart is drawn with `preserveAspectRatio="none"` and
stretches to whatever width it is given. A fraction across the box is a fraction
across the window either way, and it survives a resize with nothing recomputed.

### The defect this found

The first version kept the drag start in a Leptos `StoredValue`, and the browser
test timed out. Instrumenting a real browser showed the brush working perfectly
in isolation — so the component was right and the test was wrong, which turned
out to be exactly backwards.

`StoredValue` is created fresh by each render. A panel redraws whenever a frame
arrives, so a frame landing between pointerdown and pointerup gave the pointerup
handler a `StoredValue` that had never been written, and the drag was silently
discarded. In the probe I dragged immediately; in the test `drewSomething`
polled first, leaving room for a frame.

The same shape of bug as the resize gesture in [[HLIN-I-0002]], where pointer
capture did not survive its element being redrawn. On a shell tuned for live
data it would not be intermittent at all — at 8Hz a redraw mid-drag is close to
certain.

Fixed with a `thread_local` `Cell`, which is right rather than merely
convenient: wasm is single-threaded, a person has one pointer down at a time,
and the value is meaningless outside the moment between press and release. The
drag test went from a 31.5s timeout to 1.3s.

### Tests

Two, and the second is the one that would catch an over-eager brush: a plain
click must *not* move the surface, because below a small fraction of the width
that is somebody tapping the chart. The first asserts a preset is in force,
drags, and asserts none is — plus that the panel is still drawing on the new
window, since a range that emptied the surface would be worse than one that
never applied.

299 Rust tests, 23 browser tests (up from 21), 1 honestly skipped. Walkthrough
holds. `angreal check all` clean.

Needed two `web-sys` features on Aurora (`Element`, `DomRect`), which go
upstream with the rest of the `hlin` feature.
