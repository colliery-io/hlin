---
id: top-down-design
level: initiative
title: "Top-Down Design"
short_code: "HLIN-I-0001"
created_at: 2026-08-04T14:14:00.394383+00:00
updated_at: 2026-09-07T12:21:28.823060+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/active"


exit_criteria_met: false
estimated_complexity: L
initiative_id: top-down-design
---

# Top-Down Design Initiative

## Context

The vision ([[HLIN-V-0001]]) commits to a composition shell: platforms declare panels as data plus a view kind, the shell renders everything, and users compose surfaces themselves. It also names five open design questions — the manifest schema, the per-kind data envelope and initial vocabulary, the view registry's home, layout persistence and sharing, and authorization.

Everything in the system reads from these contracts. Building any component before they are settled means rework in every component built after. This initiative does the top-down design pass: it turns the open questions into decided, documented contracts before implementation begins.

The design has a natural dependency order. The manifest schema is upstream of everything — discovery, panels, navigation, and lifecycle all read from it. The envelope and vocabulary are upstream of the view registry. Layout persistence and authorization are downstream of both and can be decided last.

## Goals & Non-Goals

**Goals:**
- Decide the manifest schema: navigation, panel declarations, parameters, health/summary endpoints, lifecycle, versioning-and-compatibility rules. Captured as a specification; the load-bearing choices as ADRs.
- Decide the initial view-kind vocabulary and the typed data envelope for each kind, including the total panel state machine (loading / ready / stale / unavailable, with unavailability's four causes).
- Decide where the view registry lives (design system vs. companion crate) — this resolves whether `hlin-view` exists.
- Decide the crate boundaries and reshape the scaffolded workspace (`hlin-core`/`hlin-cli`) into the designed layout.
- Take positions on layout persistence/sharing and on panel-level authorization, at least to the depth needed to keep the manifest and envelope designs from foreclosing them.

**Non-Goals:**
- Implementing the shell, discovery, rendering, or any panel. This initiative produces contracts and skeletons, not features.
- Designing the visual appearance of view kinds — that belongs to the design system.
- The declarative view spec escape hatch. The envelope must not foreclose it, but it is explicitly deferred until composition-level customization fails a real case.
- Choosing the discovery transport beyond the trait boundary already committed to in the vision.

## Architecture

### System Decomposition (agreed 2026-08-04)

Five components, each the consumer of a contract this initiative designs:

| Component | Responsibility | Reads |
|---|---|---|
| **Manifest client** | Fetch, parse, validate one platform's manifest; classify failures (unreachable vs. malformed) | Manifest schema |
| **Registry** | Behind the trait committed to in the vision: the current known-platform set and their manifest sequence, with staleness; computes contract identity, diffs, and raises violations ([[HLIN-A-0002]]) | Manifest client output |
| **Aggregator** | Fan-out to panel data endpoints with forwarded identity, dedup identical requests per principal ([[HLIN-A-0004]]), deliver one stream to the browser | Validated contracts + envelopes |
| **Layout engine** | User-authored surfaces: selection, arrangement, persistence | Registry (what exists) + layout store |
| **Renderer** | Envelope + view kind → design-system component; draws the total panel state machine | Vocabulary + envelopes |

### Settled decisions

- [[HLIN-A-0001]] (decided): the aggregator owns the panel state machine. State travels in the stream protocol alongside the envelope, not inside it; envelopes are pure data; the renderer draws what it is told. The stream protocol is a second versioned contract, designed with the manifest's additive discipline.
- [[HLIN-A-0002]] (decided): panel contracts are versioned by a content hash over canonicalized contract content (did it change) plus a declared semver (was breakage intended); enforcement is runtime — the aggregator is effectively CI, the renderer is CD, and no additional service compiles or gates the composed frontend.
- [[HLIN-A-0003]] (decided): panel declarations name `kind` and `envelope` as separate fields from two separately governed vocabularies; kinds declare which envelopes they accept; invalid pairings degrade to unavailable (malformed). Refined 2026-09-07: `kind` is a default, not contract; the data contract is `envelope` + `data`. Envelope versions are permanent.
- [[HLIN-A-0004]] (decided): authentication is hoisted to Hlin; the aggregator forwards a signed identity token on every data request; platforms authorize at fetch time; dedup is keyed per principal. Closes the vision's authorization question. Adds a fifth unavailability cause, `forbidden`.
- [[HLIN-A-0005]] (decided): the view registry lives in a companion crate, `hlin-view`, depending on the design system and `hlin-manifest`. The design system stays Hlin-agnostic. Crate list is settled: `hlin`, `hlin-manifest`, `hlin-view`.
- [[HLIN-A-0006]] (decided): the shell's persistent store is Postgres from day one, behind a store trait that exists for testability only. Local dev must provide a containerized Postgres via angreal tasks.
- [[HLIN-A-0007]] (decided): layouts have one owner and are personal or org-published to a shell-wide gallery; sharing is read-only with fork to edit; forbidden panels stay visible and render as `unavailable (forbidden)`. Closes the vision's last open question.
- Panel parameters are drawn from the shared parameter vocabulary only; the manifest carries no inline parameter types. A parameter entry is a vocabulary name plus vocabulary-defined configuration that is itself data. The initial vocabulary includes a generic `select` parameter whose options come from a platform endpoint, so per-entity panels (cluster, tenant, region) do not require dynamic panel keys. (Normative in the manifest specification; no separate ADR.)

### Design review findings (2026-09-07) recorded for later phases

- **The shell is stateful.** Resolved by [[HLIN-A-0006]]: Postgres from day one.
- **Vision amendments pending.** Two sentences in [[HLIN-V-0001]] are now overstated: "fails the build that attempts it" (enforcement is runtime, per [[HLIN-A-0002]]), and "keeps the shell out of the path of vocabulary growth" (a new view kind is a design-system dependency bump and a shell redeploy; platforms never trigger shell rebuilds, but the design system does).
- **Stream-loss state.** The renderer derives exactly one transition locally, on losing its stream to the shell ([[HLIN-A-0001]] amendment). The stream-protocol specification must define the grace interval.
- **Aggregator load shaping.** Every time-range change refetches every subscribed panel for that principal; the aggregator needs coalescing policy. Not a contract concern; noted for the aggregator specification.

## Detailed Design

The approach is top-down in dependency order, with each contract producing durable artifacts as it is settled:

1. **System decomposition.** Name the components (registry, aggregator, layout engine, renderer, manifest client) and the interfaces between them, so each subsequent contract has a defined consumer.
2. **Manifest schema.** The root contract. Versioning rules (monotonic, additive, unknown-fields-ignored), the well-known path, navigation entries, panel declarations with parameters, health/summary endpoints, lifecycle status. Encoded as Rust types in `hlin-manifest` with serde round-trip tests — the crate is the spec's executable form.
3. **Vocabulary and envelopes.** The initial bounded set of view kinds, the typed envelope per kind, the shared parameter vocabulary (time range first), and the panel state machine. Admission bar for new kinds documented as part of vocabulary governance.
4. **View registry location.** ADR resolving design-system vs. companion crate, which determines the final crate list.
5. **Downstream positions.** Layout persistence/ownership and authorization, decided far enough to constrain the contracts above; each an ADR even if the decision is "platform decides, shell trusts."

Decisions are captured as ADRs at the moment they are made, not batched at the end. Specifications hold the contracts themselves; the initiative document holds only the map.

## Alternatives Considered

- **Bottom-up: build a walking skeleton first, extract contracts from it.** Rejected: the vision's whole bet is that the contracts are the product. A skeleton built before the manifest schema exists would encode a guess at it, and the guess would calcify.
- **Design everything exhaustively before any code.** Rejected in the strong form: the manifest schema and envelopes benefit from being encoded as Rust types as they are designed (serde round-trips surface schema problems prose review does not). Skeleton crates are in scope for that reason.
- **Defer layout persistence and authorization entirely.** Rejected: both constrain the manifest and envelope (an envelope with no room for per-viewer redaction forecloses one authorization model). They need positions, not implementations.

## Implementation Plan

Design work completed in discovery/design (2026-08-04 to 2026-09-07): system decomposition, [[HLIN-S-0001]] manifest schema, [[HLIN-S-0002]] vocabulary and envelopes, and ADRs [[HLIN-A-0001]] through [[HLIN-A-0007]]. All five open questions in the vision are decided.

Decomposed 2026-09-07 into eight tasks:

| Task | Scope | Depends on |
|---|---|---|
| [[HLIN-T-0001]] | Reshape workspace into `hlin`, `hlin-manifest`, `hlin-view` | — |
| [[HLIN-T-0002]] | `hlin-manifest`: manifest types, validation, canonicalization, hash, diff | T-0001 |
| [[HLIN-T-0003]] | `hlin-manifest`: envelope types, limits, validation | T-0001 |
| [[HLIN-T-0004]] | `hlin-view`: registry, acceptance matrix, state machine, totality test | T-0001, T-0003 |
| [[HLIN-T-0005]] | Stream protocol specification | — |
| [[HLIN-T-0006]] | Identity forwarding specification | — |
| [[HLIN-T-0007]] | Postgres dev tooling, store trait, v1 schema | T-0001 |
| [[HLIN-T-0008]] | Amend vision and README for ADR-2 and ADR-5 | owner approval |

Suggested order: T-0001 first; then T-0002 and T-0003 in parallel; T-0004 and T-0007 after; the two specifications (T-0005, T-0006) and the vision amendment can proceed at any time.

Exit: every open design question in the vision has a decided ADR/specification; the workspace crate layout matches the decided design and compiles; both specs have executable form with passing round-trip and totality tests; the stream and identity contracts are specified.

## Status Updates

**2026-09-07 — all eight tasks complete; ready for review.**

Every exit condition is met. All five of the vision's open questions are decided, the workspace matches the decided crate layout, both contract specifications have executable form, and the two remaining contracts are specified.

| Artifact | State |
|---|---|
| Specifications | [[HLIN-S-0001]] manifest, [[HLIN-S-0002]] vocabulary, [[HLIN-S-0003]] stream, [[HLIN-S-0004]] identity |
| Decisions | [[HLIN-A-0001]] through [[HLIN-A-0007]], all decided |
| Crates | `hlin`, `hlin-manifest`, `hlin-view`, dependency direction as designed |
| Tests | 95 by default plus a database suite; `angreal check all` clean with clippy at `-D warnings` |

Three things worth a reviewer's attention:

- **The design changed under review, twice, and both changes were improvements.** The grilling in discovery moved semver from enforced to descriptive, stopped violations from degrading panels, hoisted authentication to the shell, and added a generic selector so per-entity panels need no dynamic keys. Later, `hlin-view`'s totality test found a genuine gap in [[HLIN-S-0002]]: `options.v1` had no accepting kind, because it is read by a control rather than drawn as a panel. Envelopes gained a role and the specification was amended. A test finding a specification error is the design pass working.
- **One thing is unverified.** Docker on the development machine could not start the database, its VM disk being full, so the containerised path in [[HLIN-T-0007]] is written and reviewed but not exercised end to end. The store code itself is verified against a real Postgres 16 by another route, and CI provides its own service container. Reclaiming Docker space and running `angreal db up` closes it.
- **The specifications are still in discovery phase.** They are complete and were written to be read, but they have had no review but mine. Advancing them is the reviewer's call rather than the author's.

What this initiative deliberately did not do: implement the shell. There is no discovery loop, no registry, no aggregator, no layout engine, and nothing renders. That is the next initiative, and it now has contracts to build against rather than guesses.
