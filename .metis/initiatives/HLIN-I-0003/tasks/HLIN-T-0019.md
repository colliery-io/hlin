---
id: make-hlin-ui-generic-over-its
level: task
title: "Make hlin-ui generic over its design pack"
short_code: "HLIN-T-0019"
created_at: 2026-09-07T22:20:00+00:00
updated_at: 2026-09-07T23:24:56.847515+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# Make hlin-ui generic over its design pack

## Parent Initiative

[[HLIN-I-0003]]

## Objective

`hlin-ui` imports `DemoPack` by name and instantiates it. That one line is what
makes using any other design system an edit to Hlin. Remove it, and the
composition machinery becomes a library somebody else's binary mounts with their
own pack.

This is the blocker for everything else in the initiative.

## Acceptance Criteria


- [x] The `App` component generic over `P: DesignPack` with `P::View: IntoView`, the pack passed in rather than constructed
- [x] `hlin-ui`'s library half depends on no concrete pack; `hlin-pack-demo` moves to a dev-dependency and to whatever binary mounts the app
- [x] The demo binary in this repository still mounts `DemoPack` and still works, so the repository can demonstrate itself with no published dependency
- [x] A grep of `crates/hlin-ui/src` for `pack_demo` returns only the binary that chooses it
- [x] Checks clean, the Rust suite passing, the browser suite from [[HLIN-T-0017]] passing unchanged

## Implementation Notes

### Technical Approach
The pack is a value, not a type parameter on every component. Store it once and
read it where panels are drawn. Leptos components own their props, so the pack
needs to be cheap to clone or held behind something that is; `DemoPack` is a
unit struct today and a real pack should stay close to that.

The bound `P::View: IntoView` is where the renderer-agnostic trait meets a
Leptos frontend. `hlin-view` stays free of Leptos; the bound lives here, which
is the right place for it.

### Dependencies
None. This is the first slice.

### Risk Considerations
Threading a generic through a component tree gets noisy quickly. If it starts
touching every component, that is a signal the pack should be held in context
rather than passed, and context is the better answer.

## Status Updates

### 2026-09-07 — done, and a defect the restructure surfaced

**The pack is erased at the boundary.** `DesignPack` is generic over its view,
which is what keeps `hlin-view` free of any UI framework. That generic is useful
exactly once, where somebody chooses a pack, and a nuisance everywhere after: a
component tree threaded with `P` is one where adding a panel means touching
every signature in between. `Drawer` takes a pack and forgets it, turning the
view into an `AnyView` immediately, so `App` is the only generic thing and
nothing below it knows a design system exists.

`Arc` rather than `Rc`, and packs required to be `Send + Sync`, because Leptos
requires it of anything a reactive closure captures. Nothing is ever sent
anywhere in a client-rendered frontend, but the bound is the framework's and a
pack is almost always a unit struct.

**`hlin-ui` became a library.** It had a library half holding the DOM-free
modules and a binary half holding everything else, which meant an outside
consumer could reach the tested parts and not the app. The whole frontend is now
the library and the binary moved out, so mounting it is one line:

```rust
mount_to_body(|| view! { <App pack=DemoPack /> });
```

That is the entire content of `examples/frontend-demo`. If a consumer's binary
is longer than that, the generic work did not go far enough.

**The shell no longer hardcodes a frontend either.** It served
`crates/hlin-ui/dist` by path. A front end is a binary somebody built by
choosing a pack, so which one to serve is configuration, and `frontend` in the
config file now says so. That was not in the acceptance criteria and should have
been: it is the same hardcoding one layer out.

**A defect the browser suite found as a flake.** The remove test failed once in
fifteen and then passed. Chasing it rather than re-running found a real race:
`save` is fire-and-forget and the shell's answer replaces the whole draft, so an
older answer landing after a newer one puts back what the newer one changed.
Remove a panel while a rename is still in flight and the panel comes back.

Fixed with a write ticket. Only the newest write's answer may replace the draft,
which is the same rule the stream already follows for frames and for the same
reason. Two full browser runs clean afterwards.

Worth noting the shape of that find. The test was not looking for it, the flake
was one run in fifteen, and the honest thing was to treat a flake as a signal
rather than noise.

**What was not done.** `hlin-pack-demo` is still a dependency of `hlin` itself,
for the stylesheet the shell serves. That is [[HLIN-T-0020]] and is the next
task rather than an oversight.

Checks clean, 269 Rust tests passing, 15 browser tests passing twice.
