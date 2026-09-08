# hlin-view

The rendering vocabulary of [Hlin](https://github.com/colliery-io/hlin), and the
trait a design system implements to draw it.

Hlin is a composition shell over independently released platforms. Each platform
declares what it can show; the shell draws every panel through one design
system, so consistency follows from the architecture rather than from review.
This crate is the seam between deciding what to draw and drawing it.

## What is in here

**The vocabulary.** Six view kinds — `stat`, `timeseries`, `sparkline`, `table`,
`status` and `raw` — and the matrix saying which envelope each can draw. Kinds
change when a design system decides something should look different; envelopes
change when platforms need to say something different. Keeping them apart is
what lets each move on its own cadence.

**The state machine.** Every panel is in exactly one state at all times:
loading, ready, stale, or unavailable, with unavailability distinguishing
unreachable from malformed from unknown from deprecated. `plan` decides which,
and is total over its inputs.

**`DesignPack`.** The trait a design system implements to become a pack. Ten
methods, all required, no defaults: a pack that forgets a kind does not compile,
which is the same guarantee `plan` gives from the other side. Between them there
is no panel state and no envelope that reaches a hole.

## Implementing a pack

The trait is generic over the view it produces, so this crate takes no UI
dependency and the vocabulary stays a property of the contract rather than of
whatever draws it. A pack returning Leptos views, HTML strings or a test double
that records calls are all equally valid.

```rust,ignore
impl DesignPack for MyPack {
    type View = leptos::prelude::AnyView;

    fn stat(&self, data: &Scalar, context: Context) -> Self::View { /* ... */ }
    // ... nine more, none optional

    fn stylesheet(&self) -> &'static str { include_str!("pack.css") }
}
```

`stylesheet` is also where a pack fills the handful of CSS custom properties
Hlin's own chrome reads, so the frame around the panels matches them. See the
trait documentation for the list.

## Licence

MIT.
