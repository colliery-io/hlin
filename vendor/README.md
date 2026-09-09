# vendor

Third-party source vendored into this repository so the demo builds from a clean
clone with no registry involved.

## `aurora-leptos`

Colliery's Aurora Dark design system, published on crates.io as
`colliery-io-aurora`.

| | |
|---|---|
| Upstream | https://github.com/colliery-io/aurora-dark |
| Vendored from | `cb84a23` |
| Vendored on | 2026-09-07 |
| Why | So `examples/frontend-aurora` builds without publishing Hlin's crates first |

### This copy has been changed, and the change has to go back

A `hlin` feature was added here, off by default, which makes Aurora a Hlin
design pack: an optional `hlin-view` dependency and a module implementing
`DesignPack`. See `src/hlin.rs` and the `[features]` block in `Cargo.toml`.

**That work is now upstream.** It lived here because the crates it depends on
were not published, and iterating on an interface across two repositories while
it is still moving is how you end up with two interfaces. `hlin-view` is on
crates.io, and `aurora-dark` carries the `hlin` feature as of `58621e7`.

**This directory goes away** as soon as a release of `colliery-io-aurora`
carries the feature. Then the examples take it from the registry like anything
else, and Hlin has no path dependency left. Until that release the vendored
copy is what builds, because the published `0.1.0` predates the feature.

Anything changed here that is **not** the `hlin` feature is a mistake. There
should be nothing else, and right now there is nothing else: the source is
byte-identical to upstream and only `Cargo.toml` differs, because this copy
takes `hlin-view` by path.

`cargo fmt --all` reaches a path dependency even though this crate is excluded
from the workspace, and it once reformatted five files here against a config
upstream does not use and an edition it is not on. `rustfmt.toml` beside this
crate stops that; the workspace-level `ignore` key is nightly-only and silently
does nothing.

To see what has been changed:

```bash
diff -ru ~/path/to/aurora-dark/aurora-leptos vendor/aurora-leptos
```
