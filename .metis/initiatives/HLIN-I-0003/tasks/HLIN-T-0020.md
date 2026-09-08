---
id: put-the-stylesheet-in-the-pack
level: task
title: "Put the stylesheet in the pack contract"
short_code: "HLIN-T-0020"
created_at: 2026-09-07T22:21:00+00:00
updated_at: 2026-09-07T23:29:36.381508+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# Put the stylesheet in the pack contract

## Parent Initiative

[[HLIN-I-0003]]

## Objective

A pack that implements `DesignPack` has no way to get its CSS onto the page. The
shell serves `hlin_pack_demo::STYLESHEET` because someone wired that constant in
by hand. What a pack needs to look right is part of what a pack is, so it
belongs on the trait.

## Acceptance Criteria

- [x] A `stylesheet` method on `DesignPack` returning what the pack needs served
- [~] ~~The shell serves whatever the pack it was built with hands it, at `/assets/pack.css`, with no crate named in `server.rs`~~ — **superseded, and by something better.** The shell serves no pack CSS at all now. The route and the `<link>` to it are gone, and the app puts the pack's styling on the page itself. The shell depends on no design pack, which is stronger than not naming one
- [x] `hlin-pack-demo` implements it by returning what its constant holds today
- [x] A pack with no styling of its own is expressible without ceremony
- [x] Checks clean, the Rust suite passing, the browser suite passing unchanged

## Implementation Notes

### Technical Approach
The return type wants thought. A `&'static str` is the cheapest and covers a
pack whose CSS is `include_str!`. Aurora's case is a concatenation of three
files, which is still const-able. `Cow<'static, str>` covers a pack that builds
its stylesheet at runtime without costing anything for one that does not.

The shell holds a pack to ask it, which means the shell now knows about packs at
all. That is acceptable: it serves the file. It must not know which pack.

### Dependencies
[[HLIN-T-0019]] settles how the shell and the frontend get hold of a pack.

### Risk Considerations
Serving CSS is the one place where the shell, which is a server, needs something
from a pack, which is a frontend concern. If this gets awkward the alternative is
the pack's stylesheet being a build artifact the frontend bundles, and the shell
serving nothing. Worth reconsidering if the trait method starts pulling the
server towards Leptos.

## Status Updates

### 2026-09-07 — done, and the shell got out of the way entirely

The trait gained `stylesheet`, returning `&'static str`. Required like every
other method, because a pack whose CSS has to be wired up separately is a pack
that can be installed wrongly, and trivially satisfied by a pack with no styling
of its own returning `""`. The test double in `hlin-view`'s totality suite does
exactly that.

`&'static str` rather than the `Cow` this task proposed. Every pack anybody has
written has a compile-time stylesheet: an `include_str!`, or a `concat!` of
several, which is what Aurora's will be. A pack that genuinely builds CSS at
runtime would need this widened and none does, so the simpler type wins until
something asks for the other.

**The bigger change is that the shell stopped serving pack CSS at all.** The
task's risk note called it: "if this gets awkward the alternative is the pack's
stylesheet being a build artifact the frontend bundles, and the shell serving
nothing." It was awkward for a reason worth naming. The shell is a server; a
pack is a frontend concern; the only reason the shell was ever told about one
was that the frontend had no way to hold a pack. [[HLIN-T-0019]] gave it one.

So `/assets/pack.css` is gone, the `<link>` to it is gone from the page, and the
app puts the pack's styling on the page itself with a `<style>` element. This is
the same thing Aurora already does with `AuroraStyles`, and the flash it warns
about does not apply: a client-rendered page shows nothing until the WebAssembly
mounts, so there is no unstyled moment to flash through.

The result is the one that matters for this initiative. `hlin`, the shell
binary, no longer depends on any design pack at all. It serves data and whatever
frontend it is pointed at, and has no way of knowing what drew it.

Checks clean, 269 Rust tests passing, 15 browser tests passing.
