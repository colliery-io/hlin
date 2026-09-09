# hlin-ui

The browser half of [Hlin](https://github.com/colliery-io/hlin): the surface a
person composes, watches and shares. Everything except how the panels look.

Hlin is a composition shell over independently released platforms. Each platform
declares what it can show; the shell draws every panel through one design
system, so consistency follows from the architecture rather than from review.

## Making a front end

A front end is a binary. It picks a design pack and mounts this app, and that is
the whole of it — the picker, the grid, the stream, the time controls, the
degradation states and the return path all come from here, and none of it knows
which design system is drawing.

```rust
use hlin_ui::app::App;
use leptos::prelude::*;
use your_pack::YourPack;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| view! { <App pack=YourPack /> });
}
```

`YourPack` is anything implementing `DesignPack` from
[`hlin-view`](https://crates.io/crates/hlin-view). Ten methods, no defaults: a
pack that forgets a view kind does not compile.

Build it with [Trunk](https://trunkrs.dev), which produces a `dist` directory.

## Serving it

The shell serves whichever directory its `frontend` setting points at, so a
front end is a deployment artefact rather than a fork. On Kubernetes, put `dist`
in an image and name it:

```dockerfile
FROM busybox
COPY dist /dist
```

```sh
helm install hlin oci://ghcr.io/colliery-io/charts/hlin \
  --set config.frontendImage=ghcr.io/you/your-pack:1.0
```

An init container copies it over the front end the shell image shipped with. The
shell image stays upstream's, so a fix to the shell is a tag bump rather than
your rebuild.

## What this crate does not decide

Which panels exist, what they mean, how often they refresh, or what a click
does. Panels come from platforms, cadence is declared by the panel and clamped
by the shell, and a click returns a value from a closed vocabulary — `Select`,
`Range`, `Emit`, `Control` — which the shell turns into a question for a
platform. A pack draws; it does not get to invent interactions the shell cannot
carry.

## License

MIT
