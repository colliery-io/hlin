# Aurora Dark — patterns & usage guide

How to choose and compose `aurora-leptos` pieces. Written for both people and AI
agents building Colliery Leptos UIs. Read **Core model** first, then use the
**Pick by intent** table to jump to a component, and **Choosing between similar
pieces** when two options look alike.

---

## Core model (read first)

1. **One stylesheet.** Load it once, one of two ways (see the README's "Styling"
   for setup): runtime injection via `<AuroraStyles/>` (simplest; possible flash),
   or a `<link>`ed `style/aurora.css` for no flash (emit it from `build.rs` under
   cargo-leptos, or a trunk `pre_build` hook). Without one, components render unstyled.
2. **Dark only.** There is no light theme. Surfaces/text/accents come from CSS
   custom properties (`--bg`, `--panel`, `--fg`, `--ice`, …).
3. **Use tokens, not raw hex.** In Rust use `token::ICE` etc.; in CSS use
   `var(--ice)`. Status meaning comes from `status_color(&str)`. Never hardcode
   `#7fb2ff`.
4. **The pack renders; the app supplies meaning.** Data-driven values — a status
   color, a state label, a tooltip string, a brand mark — are passed in as props.
   The pack ships no app vocabulary or logo. (e.g. a `HealthPill` takes
   `label`/`color`/`tip`; *you* decide that "live" is green.)
5. **Renderer-agnostic.** The crate depends on `leptos` with no renderer feature;
   your binary selects `csr` (or `hydrate`/`ssr`).
6. **Reactivity.** Inputs bind to `RwSignal`s (two-way); handlers are `Callback`s.
   `Modal`/`Menu` open-state are `RwSignal<bool>`.

```rust
use aurora_leptos::{AuroraStyles, components::*, widgets::*, graph::*, tokens::token};
```

---

## Pick by intent

| I want to… | Use | Notes |
|---|---|---|
| Lay out a horizontal row | `Group` | `gap`, `justify="between"`, `wrap` |
| Stack things vertically | `Stack` | `gap`, `center` |
| Equal-width grid | `SimpleGrid` | `cols=N` |
| Proportional 12-col grid | `Grid` + `GridCol span=` | when columns differ in width |
| App scaffold (nav + header + body) | `AppShell` | top-level page chrome |
| Page title + subtitle + actions | `PageHeader` | `right=` slot for buttons |
| A bordered content card | `Panel` | `title` + optional `caption` |
| Body / caption / mono text | `Text` | `mono`, `dimmed`, `bright`, `bold`, `size` |
| Inline code / a link | `Code` / `Anchor` | — |
| A button | `Button` | `variant`, `size`, `bad`, `on_click` |
| An icon-only button | `ActionIcon` | give it a `title` |
| Text / number / password / multiline field | `TextInput` / `NumberInput` / `PasswordInput` / `Textarea` | bind `value` to an `RwSignal` |
| Pick one of a few options | `Select` (many) · `SegmentedControl` (2–4) | — |
| An on/off toggle | `Switch` | `checked: RwSignal<bool>` |
| Copy-to-clipboard | `CopyButton` | `value` |
| A hover explanation | `Tooltip` | wrap the trigger |
| A confirming / focused overlay | `Modal` | `open: RwSignal<bool>` |
| A dropdown of actions | `Menu` + `MenuItem` | items close it on click |
| A short status tag | `StatusBadge` (status string) · `Pill` (custom hue) · `HealthPill` (+tooltip) | see comparison below |
| A small status dot | `Dot` | `color`, `glow` |
| A filter toggle | `Chip` | `active: Signal<bool>` |
| Loading / empty / error states | `Loading` / `Empty` / `ErrorState` | every async view should use these |
| An inline notice / callout | `Banner` (transient) · `Alert` (in-content) | — |
| Counts per state | `StateCounts` | `Vec<StateCount{label,count,color}>` |
| A freshness / progress bar | `Meter` | `value` 0–100 |
| Build/CI status | `BuildStatusBadge` | success/building/failed/pending |
| A data table | `Table` (generic) · `InputTable` (sources + freshness) | see comparison |
| Readiness of N inputs | `NodeReadiness` | trigger summary + per-input freshness |
| Warn about stale sources | `StaleInputsBanner` | renders nothing when all fresh |
| Draw a graph / DAG | `Graph` | auto-layout from nodes + edges |

---

## Choosing between similar pieces

- **`StatusBadge` vs `Pill` vs `HealthPill` vs `Chip`** — `StatusBadge` when you
  have a status *string* and want the standard `status_color` mapping
  (running/completed/failed/…). `Pill` when you choose the hue yourself
  (`color=token::VIOLET`). `HealthPill` when the tag needs a hover tooltip
  explaining the state. `Chip` only for interactive *filters* (it's clickable).
- **`Banner` vs `Alert` vs `ErrorState`** — `ErrorState` for a failed async load
  (takes an `ApiError`, renders the right title/retry by kind). `Alert` for an
  in-content callout tied to a section. `Banner` for a page-level transient notice
  (full-width, accent + icon). `StaleInputsBanner` is a ready-made `Banner` for
  the "some sources are stale" case.
- **`Table` vs `InputTable`** — `Table` is generic markup (`<thead>/<tbody>` you
  write). `InputTable` is purpose-built for a list of data **inputs/sources** with
  state, last-event, rate, a freshness `Meter`, and a per-row action.
- **`SimpleGrid` vs `Grid`** — `SimpleGrid cols=N` for equal columns (cards,
  stats). `Grid` + `GridCol span=` (out of 12) when widths differ.
- **`Group`/`Stack` vs grids** — `Group`/`Stack` for a handful of inline items
  (flex); grids for tabular/cellular layouts that should wrap evenly.
- **`Modal` vs `Menu`** — `Modal` for a focused task/confirmation that blocks the
  page; `Menu` for a small list of actions off a trigger button.

---

## Reference by area

Key props only — see rustdoc for the full signatures.

### Layout
`Box` · `Group{justify,top,wrap,gap}` · `Stack{center,gap}` ·
`SimpleGrid{cols}` · `Grid` + `GridCol{span}` · `Divider` ·
`AppShell{header?, navbar, children}`.

### Typography
`Text{size,dimmed,bright,bold,mono}` · `Code` · `Anchor{href}` ·
`List` + `ListItem`. The `MONO`/tabular look comes from `mono=true` or the
`.cl-mono` class.

### Inputs (bind `value`/`checked` to an `RwSignal`)
`Button{variant,size,bad,disabled,on_click}` · `ActionIcon{title,on_click}` ·
`TextInput{label,placeholder,value,error}` · `Textarea{…,rows}` ·
`PasswordInput{…}` · `NumberInput{label,value:RwSignal<f64>,step}` ·
`Select{label,options:Vec<String>,value}` · `Switch{checked,label}` ·
`SegmentedControl{options,value}` · `CopyButton{value}`.

```rust
let name = RwSignal::new(String::new());
view! { <TextInput label="Name" value=name placeholder="e.g. nightly" /> }
```

### Overlays & feedback
`Tooltip{label}` · `Modal{open:RwSignal<bool>,title}` · `Menu{label}` + `MenuItem{on_click}` ·
`Alert{title,color}` · `Loader`.

```rust
let open = RwSignal::new(false);
view! {
    <Button on_click=Callback::new(move |_| open.set(true))>"Open"</Button>
    <Modal open=open title="Confirm"> <Text>"…"</Text> </Modal>
}
```

### Status & async states
`Pill{color}` · `StatusBadge{status}` · `Dot{color,size,glow}` ·
`Chip{label,count,active,on_click}` · `Loading{label}` · `Empty{message}` ·
`ErrorState{error:ApiError,on_retry}`.

```rust
// Async view shape: never a blank screen.
match resource.get() {
    None => view! { <Loading/> }.into_any(),
    Some(Err(e)) => view! { <ErrorState error=e on_retry=retry/> }.into_any(),
    Some(Ok(items)) if items.is_empty() => view! { <Empty message="Nothing here."/> }.into_any(),
    Some(Ok(items)) => /* render */,
}
```

### Data-display widgets
`Meter{value,color}` · `Banner{color,icon}` · `StaleInputsBanner{inputs}` ·
`StateCounts{counts}` · `HealthPill{label,color,tip}` · `BuildStatusBadge{status}` ·
`NodeReadiness{node,mode_label,strategy_label,require_all,inputs,last_run_at}` ·
`InputTable{inputs,on_action,action_label}`.

The `Input` model is display-oriented — the app fills `state_label`/`state_color`
(its own vocab), `rate` (pre-formatted), and timestamps (`f64` ms). `is_stale` /
`freshness_pct` / `format_ago` are time helpers you can reuse.

### Graph / DAG
`Graph{nodes:Vec<GraphNode>, edges:Vec<GraphEdge>, direction}` with a built-in
layered layout (`"TB"` default, or `"LR"`). Build nodes/edges fluently:

```rust
let nodes = vec![
    GraphNode::new("a", "orders").color(token::ICE).sublabel("source"),
    GraphNode::new("b", "rollup").color(token::VIOLET),
];
let edges = vec![ GraphEdge::new("a", "b").active(true) ];
view! { <Graph nodes=nodes edges=edges /> }
```

Use `Graph` when nodes are few–dozens and a layered layout reads well. For large
graphs or when you need crossing-minimisation, compute positions with a dedicated
layout crate (`layout-rs`/`rust-sugiyama`) and render — `layout_dag` is exposed if
you want the built-in positions for custom rendering.

---

## Tokens

- **Surfaces**: `--bg --sidebar --panel --panel-2 --inset --control --border --border-soft --edge`
- **Text**: `--fg --fg-bright --fg-2 --muted --faint`
- **Accents/status**: `--ice --teal --violet --gold --ok --bad --skip` (Rust: `token::*`)
- **Scales**: `--space-{xs..xl}`, `--radius-{xs..xl}` + `--radius-pill/panel/chip`,
  `--fs-{xs..xl}`, `--h-{xs,sm,md}` (control heights), `--font-sans`/`--font-mono`.

Data-driven color goes inline; static chrome uses the `.cl-*` classes. To color a
status, call `status_color(s)` (or your own map) and pass it as a prop.

---

## Checklist for agents

1. Add the dep + a renderer feature in the binary; `use aurora_leptos::…`.
2. Render `<AuroraStyles/>` once (or link the CSS files).
3. Reach for an existing component via **Pick by intent** before writing markup.
4. Bind inputs to `RwSignal`s; pass handlers as `Callback`s.
5. Use `token::*` / `var(--…)` and `status_color` — never invent classes or hex.
6. Supply app-specific labels/colors/branding as **data**; don't add them to the pack.
7. Wrap async UI in `Loading`/`Empty`/`ErrorState`.
