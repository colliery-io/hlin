---
id: one-frontend-two-design-systems
level: task
title: "One frontend, two design systems, chosen at runtime"
short_code: "HLIN-T-0025"
created_at: 2026-09-08T00:30:00+00:00
updated_at: 2026-09-08T00:18:34.609225+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# One frontend, two design systems, chosen at runtime

## Parent Initiative

[[HLIN-I-0003]]

## Objective

Two frontends differing by one identifier is an argument. One frontend holding
both packs, where a person changes the design system in the address bar and the
same surface redraws, is a demonstration.

`DesignPack` keeps object safety despite its associated type, which
[[HLIN-T-0024]] pinned with a test but nothing exercises. This makes that
property do something a person can see.

## Acceptance Criteria


- [x] `DesignPack` implemented for a boxed pack, so a trait object satisfies the same bound a concrete pack does and `App` needs no second entry point
- [x] A frontend holding both packs, choosing between them at runtime, defaulting sensibly when nothing is asked for
- [x] Only the chosen pack's stylesheet reaches the page, so the two do not fight
- [x] Built by the angreal `ui` group like the others, and servable by pointing the shell's `frontend` at it
- [x] The browser suite passes against it, unchanged, because it asserts nothing about any pack
- [x] Screenshots of the same surface under both packs, looked at
- [x] Written down plainly that this is a demonstration and not a deployment pattern: it compiles two design systems into one binary and pays for both

## Implementation Notes

### Technical Approach
`Box<dyn DesignPack<View = V>>` does not implement `DesignPack` on its own, so
either the box gets an impl that forwards, or `App` grows a second way in. The
forwarding impl is the smaller change and keeps one entry point, which is worth
more than saving ten lines.

Selection belongs to the viewer rather than the shell. The shell serves data and
a frontend and has no business knowing which design system was compiled in; a
query parameter is the frontend's own affair.

### Dependencies
[[HLIN-T-0019]] for the generic app, [[HLIN-T-0023]] for a second pack to choose
between.

### Risk Considerations
This is the one place in the repository where making a demo better could make
the product worse, by suggesting runtime pack selection is a thing to build on.
It is not: two design systems in one bundle is a cost nobody should pay in
production. Say so where somebody would read it.

## Status Updates

### 2026-09-08 — the same surface, two design systems, one binary

`examples/frontend-gallery` holds both packs and reads `?pack=` when the page
loads. `?pack=aurora` and `?pack=demo` draw the same layout, the same data and
the same composition machinery, with every panel redrawn by a different design
system. A name nobody offers falls back to the first rather than showing a blank
page, which is the rule the vocabulary already uses for an unknown view kind.

**Nothing in `hlin-ui` changed.** That is the finding. `App<P>` accepted a
`Box<dyn DesignPack<View = AnyView> + Send + Sync>` with no new entry point,
because the box implements the trait it is a box of. Eleven lines of forwarding
in `hlin-view` bought a runtime choice, and the composition machinery cannot
tell the difference between a concrete pack and a boxed one.

**Only the chosen pack's stylesheet reaches the page**, which fell out rather
than being arranged: `Drawer` reads `stylesheet()` from the pack it was given,
so the other one's CSS is compiled in and never emitted. The chrome tokens
follow, so the frame is light under the demo pack and dark under Aurora in the
same binary.

**The cost, measured rather than asserted:**

| Front end | Bundle |
|---|---|
| `frontend-demo` | 4844 KB |
| `frontend-aurora` | 5076 KB |
| `frontend-gallery` | 5848 KB |

Roughly a megabyte for the second design system, all of it dead weight for any
given viewer. That is why this is an example and why the doc comment at the top
of it says so in its own paragraph. A real front end picks one, as the other two
do, and they differ from each other by a single identifier.

**What it demonstrates that two binaries could not.** Two binaries are an
argument that the seam is clean. One binary where a person changes the design
system in the address bar and watches the same panels redraw is the seam being
clean, visibly, without a rebuild. It also exercises the object safety that
[[HLIN-T-0024]] pinned with a test and nothing used.

The browser suite passes against it unchanged, which is worth noting: fifteen
tests written against no pack in particular now run against three front ends.

Checks clean, 270 Rust tests passing, 15 browser tests passing against the
gallery.
