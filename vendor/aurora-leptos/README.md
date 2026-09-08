# aurora-leptos

**Colliery's general dark design system for [Leptos](https://leptos.dev).** Aurora
Dark — semantic tokens, a complete set of UI components + data-display widgets, and
the stylesheet — the **core that control-plane apps are built from** (cloacina
included). Everything is first-class core; nothing is a gated optional add-on. Only
genuinely app-specific surfaces (e.g. cloacina's DAG/graph + node views) are built
downstream from these primitives.

Published on crates.io as **`colliery-io-aurora`**; the library is imported as
`aurora_leptos`.

```toml
[dependencies]
colliery-io-aurora = "0.1"                               # or { path = "../aurora-leptos" } in-workspace
leptos = { version = "0.8", features = ["csr"] }         # the binary picks the renderer
```

```rust
use aurora_leptos::{AuroraStyles, components::*, tokens::token};
use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    view! {
        <AuroraStyles/>                       // or <link> the style/*.css files
        <PageHeader title="My App" sub="leptos · wasm"/>
        <Button>"Run"</Button>
        <StatusBadge status="running"/>
        <Pill color=token::ICE>"tag"</Pill>
    }
}
```

## Generic core (always available)
- **Layout** — `Box` · `Group` · `Stack` · `SimpleGrid` · `Grid`/`GridCol` ·
  `Divider` · `AppShell`.
- **Typography** — `Text` (+ `mono`) · `Code` · `Anchor` · `List`/`ListItem`.
- **Inputs** — `Button` · `ActionIcon` · `TextInput` · `Textarea` · `PasswordInput` ·
  `NumberInput` · `Select` · `Switch` · `SegmentedControl` · `CopyButton`.
- **Data / overlay** — `Table` · `Tooltip` · `Modal` · `Menu`/`MenuItem` · `Alert` ·
  `Loader`.
- **Aurora** — `Pill` · `StatusBadge` · `Dot` · `Panel` · `PageHeader` · `Chip` ·
  `Loading` · `Empty` · `ErrorState`.
- **tokens** — semantic palette (`token::*`), `status_color`, `pill_bg`, and
  `classify` over an `ApiError`. Pure, framework-agnostic Rust.

## Widgets (`widgets`, also core)
Generic, higher-level data-display building blocks. Vocabulary is generic dataflow
— a **node** processes when its **inputs** are ready:
`Meter` · `Banner` · `StateCounts` · `HealthPill` · `BuildStatusBadge` ·
`NodeReadiness` · `InputTable` · `StaleInputsBanner`, plus the `Input` model and
`format_ago`/`is_stale`/`freshness_pct` helpers. **Apps supply their own state
labels, colors, tooltips, and branding as data** — the pack ships no app vocab,
palette, or logo. App-specific views (e.g. cloacina's reactors→nodes,
accumulators→inputs, and its DAG/graph + node views) are built downstream from
these. See `../leptos-gallery` for a worked example.

## Styling: pick one
The stylesheet ships inside the crate; inject it at runtime or materialise it as a
file at build time. `write_css` / the `aurora-css` bin are leptos-free
(`default-features = false`), so the build step never compiles leptos for the host.

1. **Runtime (simplest):** `<AuroraStyles/>` once at the app root — `include_str!`s
   the CSS into the wasm. Possible first-paint flash.
2. **Linked stylesheet, no flash:**
   - **trunk** validates assets before building, so `build.rs` is too late — emit
     the file in a `pre_build` hook running the `aurora-css` helper, then
     `<link data-trunk rel="css" href="style/aurora.css">`. (Install it with
     `cargo install colliery-io-aurora --no-default-features --features bin`; in
     a workspace, `cargo run -p colliery-io-aurora … --bin aurora-css`. See
     `../leptos-gallery/Trunk.toml`.)
   - **cargo-leptos** builds before bundling styles, so calling
     `write_css(Path::new("style"))` from `build.rs` works; point `style-file` at it.

See the workspace `../README.md` for full snippets.

**When to use what:** see [`PATTERNS.md`](./PATTERNS.md) — a usage guide (for people
and AI agents) with a pick-by-intent table and how to choose between similar pieces.

See `../leptos-gallery` for a working example rendering every component and widget.

## Status
Component coverage is **complete** — primitives + widgets (see `../INVENTORY.md`),
exercised in `../leptos-gallery`. The standard for Colliery Leptos UIs.
