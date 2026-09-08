# Hlin

One vantage over many systems, assembled by the people who use them.

Hlin is a composition shell. Each platform declares, at runtime, what it can
show; Hlin discovers those declarations, renders every declared panel through a
single design system, and lets a person select panels from any platform and
arrange them into a surface they authored.

## Quick Start

```bash
cargo install --path crates/hlin
hlin --help
```

## Crates

| Crate | Responsibility |
|---|---|
| `hlin` | The shell binary: discovery, registry, aggregation, layouts, store |
| `hlin-manifest` | The contract crate: manifest schema, panel declarations, lifecycle, envelopes |
| `hlin-view` | The view registry: kinds, the kind/envelope acceptance matrix, rendering |

## Development

See the [Getting Started](getting-started/installation.md) guide.
