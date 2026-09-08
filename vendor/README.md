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

**That work belongs upstream.** It is here because the crates it depends on are
not published yet, and iterating on an interface across two repositories while
it is still moving is how you end up with two interfaces. Once `hlin-view` and
`hlin-manifest` are released, the feature should be contributed to `aurora-dark`
and this directory deleted.

Until then this copy will drift from upstream, and anything changed here that is
**not** the `hlin` feature is a mistake. There should be nothing else.

To see what has been changed:

```bash
diff -ru ~/path/to/aurora-dark/aurora-leptos vendor/aurora-leptos
```
