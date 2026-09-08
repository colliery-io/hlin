---
id: vocabulary-and-envelopes
level: specification
title: "Vocabulary and Envelopes"
short_code: "HLIN-S-0002"
created_at: 2026-09-07T11:38:17.737798+00:00
updated_at: 2026-09-07T11:38:17.737798+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Vocabulary and Envelopes

## Overview

The manifest ([[HLIN-S-0001]]) names things it does not define: view kinds, envelopes, parameters, and icons. This specification defines them. It fixes the initial bounded set of each, the compatibility mapping between kinds and envelopes, the encoding of parameters onto data requests, the panel state machine every kind must render, and the admission bar for growing any of the sets.

Three vocabularies, three cadences:

| Vocabulary | Owner | Cadence | Growth rule |
|---|---|---|---|
| **View kinds** | Design system | Slow, deliberate | New kind = design-system release + shell redeploy. Bounded on purpose |
| **Envelopes** | Wherever the view registry lives (pending ADR) | Platform speed | Additive within a version; new version for incompatible shape; versions are permanent ([[HLIN-A-0003]]) |
| **Parameters** | Shell | Rare | New parameter = shell release; each defines its own configuration and request encoding |

Icons are names from the design system's icon set and are not enumerated here; an unknown icon name renders the design system's default glyph.

## System Context

### Actors
- **Platform teams**: choose an envelope for each panel, produce documents of that shape, and name a default kind and any parameters. They never see how a kind is drawn.
- **Aggregator (shell)**: validates every returned document against its declared envelope, applies size limits, encodes parameter values onto requests, and drives the panel state machine ([[HLIN-A-0001]]).
- **Renderer (shell)**: a total function from (state, envelope, kind) to a design-system component. Never inspects an envelope it was not told the type of.
- **Design system**: owns kinds, their rendering, and the kind→envelope acceptance matrix.

### Boundaries
Inside scope: the definitions above and the rules for changing them. Outside scope: the wire framing of the stream (stream-protocol specification), the visual design of any kind, the declarative view-spec escape hatch (deferred per the vision), and where the view registry lives (pending ADR; this specification is agnostic).

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | Every envelope document carries a top-level `"envelope"` field naming its own type and version; a document whose self-declaration does not match the panel's declared envelope is malformed | The shell validates what it received against what was promised, not what it assumed |
| REQ-1.2 | Envelopes carry data and data semantics only: values, units, labels, timestamps. No colors, thresholds, sizes, or layout hints | The shell renders everything; thresholds are composition-level customization (vision) |
| REQ-1.3 | Unknown fields in an envelope document are ignored. Within a version, envelope evolution is additive only; an incompatible change is a new version, and old versions are never retired | Platforms are never forced to upgrade ([[HLIN-A-0003]]) |
| REQ-1.4 | Every kind accepts at least one envelope, and one kind, `raw`, accepts every envelope. A panel whose kind is unknown to the shell renders as `raw` | Totality: no envelope exists without a rendering, no kind name breaks a layout |
| REQ-1.5 | Every kind defines a rendering for every panel state, including all unavailability causes | Rendering is total over its inputs (vision) |
| REQ-2.1 | Parameter values are encoded onto the `data` request as query parameters, per the encoding each parameter defines. Query names `from`, `to`, and `step` are reserved for `time_range` | One encoding rule; platforms parse ordinary query strings |
| REQ-2.2 | Relative time expressions are resolved to absolute instants by the shell before encoding; platforms never see `now-1h` | Platforms implement one thing; the shell owns the clock the user sees |
| REQ-3.1 | Over-limit documents are rejected as malformed with an operator signal, never silently truncated | Silent truncation makes a panel lie; the `step` hint exists so platforms can downsample |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | Every envelope in this specification has reference Rust types in `hlin-manifest` with serde round-trip tests for each example here | The crate is the spec's executable form |
| NFR-1.2 | Adding a kind, envelope, or parameter never changes the meaning of an existing one | Additive vocabularies are the only kind platforms can depend on |

## Envelopes

### Common rules

Every envelope is a JSON object with a required `envelope` field. All other fields are as defined per envelope. Timestamps are RFC 3339 strings except inside `series.v1` points, where they are integer epoch milliseconds for compactness. An optional `as_of` (RFC 3339) on any envelope states when the data was true; the aggregator uses it, when present, in preference to fetch time for staleness. Numbers are JSON numbers; `null` is permitted wherever a value is absent.

`unit` is an optional string on `scalar.v1`, `series.v1`, and `records.v1` columns, drawn from a small set the design system formats: `count`, `percent`, `bytes`, `bytes_per_second`, `seconds`, `milliseconds`, `per_second`, or any other string, which is displayed verbatim as a suffix.

### `scalar.v1` — one value

```json
{ "envelope": "scalar.v1", "value": 1523, "unit": "per_second", "label": "records/s", "previous": 1490, "as_of": "2026-09-07T11:30:00Z" }
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `value` | number \| string \| boolean \| null | yes | |
| `unit` | string | no | |
| `label` | string | no | Short caption under the value |
| `previous` | number | no | A comparison value; the kind decides how to show the delta |

### `series.v1` — one or more time series

```json
{
  "envelope": "series.v1",
  "unit": "per_second",
  "series": [
    { "name": "worker-a", "points": [[1757244600000, 12.5], [1757244660000, 13.1], [1757244720000, null]] },
    { "name": "worker-b", "points": [[1757244600000, 9.0],  [1757244660000, 9.4],  [1757244720000, 9.9]] }
  ]
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `series[]` | array, ≥ 1 | yes | |
| `series[].name` | string | yes | Unique within the document |
| `series[].points[]` | array of `[epoch_ms, number \| null]` | yes | Ascending by time; `null` is a gap |
| `unit` | string | no | Applies to all series |

Limit: 20 series, 5 000 points per series.

### `records.v1` — tabular data

```json
{
  "envelope": "records.v1",
  "columns": [
    { "key": "stage", "label": "Stage", "type": "string" },
    { "key": "depth", "label": "Depth", "type": "number", "unit": "count" },
    { "key": "oldest", "label": "Oldest", "type": "timestamp" }
  ],
  "rows": [
    { "stage": "parse", "depth": 41, "oldest": "2026-09-07T11:29:12Z" },
    { "stage": "index", "depth": 3,  "oldest": "2026-09-07T11:30:40Z" }
  ]
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `columns[]` | array, ≥ 1 | yes | Display order |
| `columns[].key` | string | yes | Unique; row object key |
| `columns[].label` | string | yes | |
| `columns[].type` | `string` \| `number` \| `boolean` \| `timestamp` \| `duration_ms` | yes | Drives formatting and sorting |
| `columns[].unit` | string | no | For `number` columns |
| `rows[]` | array of objects keyed by column key | yes | Missing keys render empty; unknown keys ignored |

Limit: 50 columns, 1 000 rows.

### `status.v1` — health of one thing or a list of things

```json
{
  "envelope": "status.v1",
  "status": "degraded",
  "label": "Ingest",
  "detail": "1 of 4 workers restarting",
  "since": "2026-09-07T11:02:00Z",
  "items": [
    { "name": "worker-a", "status": "ok" },
    { "name": "worker-b", "status": "down", "detail": "restarting" }
  ]
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `status` | `ok` \| `degraded` \| `down` \| `unknown` | yes | The rollup |
| `label` | string | no | |
| `detail` | string | no | One line |
| `since` | RFC 3339 | no | When the current status began |
| `items[]` | array of `{ name, status, detail? }` | no | Constituent parts, same status set |

Limit: 200 items. This envelope is how at-a-glance summaries are expressed; the manifest has no separate summary endpoint.

### `options.v1` — choices for a `select` parameter

```json
{
  "envelope": "options.v1",
  "options": [
    { "value": "us-east", "label": "US East", "group": "Production" },
    { "value": "eu-west", "label": "EU West", "group": "Production" },
    { "value": "lab",     "label": "Lab" }
  ]
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `options[]` | array | yes | May be empty; the kind renders "no options" |
| `options[].value` | string | yes | Unique; what is sent back |
| `options[].label` | string | yes | |
| `options[].group` | string | no | Visual grouping only |

Limit: 500 options. Fetched with forwarded identity ([[HLIN-A-0004]]), so options may differ per principal; cached per principal by the aggregator.

### Size limit

Any envelope document over 1 MiB is malformed regardless of the per-envelope limits above.

## View Kinds

| Kind | Accepts | Renders |
|---|---|---|
| `stat` | `scalar.v1` | The value, formatted by unit; the label; the delta against `previous` if present |
| `timeseries` | `series.v1` | Line chart with time axis; legend from series names; gaps for `null` |
| `sparkline` | `series.v1` | Compact line per series with the latest value; no axes |
| `table` | `records.v1`, `series.v1` | Sortable table. For `series.v1`, one row per timestamp, one column per series |
| `status` | `status.v1` | Rollup badge with label and detail; an item list when `items` is present |
| `raw` | any | The panel title, the envelope name, and the document as a formatted key/value tree. The fallback for unknown kinds and the guaranteed rendering for every envelope |

The kind named in a manifest is the default. A user may switch a panel in their layout to any other kind that accepts the panel's envelope; that choice is layout state, not manifest state ([[HLIN-A-0003]]).

### Envelope roles (amended 2026-09-07)

Found while implementing [[HLIN-T-0004]]: the totality test asserted the governance rule below, that every envelope has an accepting kind other than `raw`, and `options.v1` failed it. The rule was right and the vocabulary was under-specified, because `options.v1` is not the same kind of thing as the others.

Envelopes divide by what reads them:

| Role | Envelopes | Read by |
|---|---|---|
| **Panel** | `scalar.v1`, `series.v1`, `records.v1`, `status.v1` | A panel's data endpoint returns one; a view kind draws it |
| **Parameter** | `options.v1` | A `select` control's options endpoint returns one; the shell renders it as a chooser |

A panel declaring a parameter envelope is rejected as malformed, since no kind draws one meaningfully. The governance rule requiring an accepting kind other than `raw` applies to panel envelopes only. `raw` still accepts every envelope regardless of role, so nothing that reaches a renderer is undrawable.

## Parameters

### `time_range` (no configuration)

The shell's global time picker. Encoded as `from=<RFC 3339>&to=<RFC 3339>&step=<integer seconds>`. `from` and `to` are absolute instants (REQ-2.2). `step` is a hint computed by the shell from the range and the panel's rendered width; a platform serving `series.v1` should downsample to about that resolution and may ignore it otherwise.

### `select` (configuration required)

A panel-scoped chooser. Manifest configuration:

| Field | Type | Required | Notes |
|---|---|---|---|
| `id` | string, same pattern as a panel key; must not be `from`, `to`, or `step` | yes | Query parameter name |
| `label` | string | yes | Control caption |
| `options` | relative path | yes | Endpoint returning `options.v1` |
| `multiple` | boolean | no (default `false`) | |

Encoded as `<id>=<value>`, repeated once per value when `multiple`. When the user has made no selection, the parameter is omitted and the platform applies its own default. The selection is layout state, persisted with the panel instance.

## Panel State Machine

Owned by the aggregator ([[HLIN-A-0001]]); every kind renders every state (REQ-1.5).

| State | Meaning | Rendering rule |
|---|---|---|
| `loading` | No envelope received yet for the current request | Kind-specific skeleton |
| `ready` | Latest envelope is fresh | The kind's normal rendering |
| `stale` | Last envelope is older than the staleness window, or the stream to the shell is lost | The last envelope, visibly aged, with its age |
| `unavailable (unreachable)` | Platform (or, on stream loss past the grace interval, the shell) not responding | Last envelope if any, dimmed, with reason; otherwise a placeholder with reason |
| `unavailable (malformed)` | Manifest declaration invalid, envelope invalid, over limit, or pairing not allowed | Placeholder with reason; no data shown |
| `unavailable (unknown)` | Panel or platform no longer exists in the registry | Placeholder with reason and, if the layout stored one, the successor hint |
| `unavailable (deprecated)` | Past the panel's sunset | Placeholder with reason and successor |
| `unavailable (forbidden)` | Platform returned 403 for this principal ([[HLIN-A-0004]]) | Placeholder with reason; no data shown |

Transitions: `loading → ready | unavailable(*)`; `ready → stale` on staleness window or stream loss; `stale → ready` on fresh envelope; `stale → unavailable(unreachable)` after the grace interval; any state `→ unavailable(malformed | forbidden)` on the corresponding fetch outcome; any state `→ unavailable(unknown | deprecated)` on registry change; `unavailable(*) → loading` on retry or manifest change. Thresholds and intervals are shell configuration; wire framing is the stream-protocol specification.

## Governance

**Admitting a view kind** requires: a named owner in the design system; at least one accepted envelope; a defined rendering for every state above; a statement of what composition-level customization it supports (title, thresholds, and the like); and a reason it cannot be an existing kind with a different envelope. Kinds are removed never; a kind may be marked legacy, and the shell keeps rendering it.

**Admitting an envelope** requires: a declared role, panel or parameter; for a panel envelope, at least one accepting kind other than `raw`; a self-declaring `envelope` name with version suffix; per-field types and limits; an example that round-trips in `hlin-manifest`; and, if it is a new version of an existing envelope, a statement of what could not be done additively. Envelope versions are permanent.

**Admitting a parameter** requires: a defined configuration schema, a query encoding that does not collide with reserved names, and a statement of whether its value is layout state or global shell state.

**Candidates deliberately not in v1**, to be admitted only against a real case: `text.v1` (a paragraph, no markup), `distribution.v1` (histogram buckets), `links.v1` (a list of deep links), and a `status-grid` kind. Each is plausible; none has a demanding panel yet.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0001]] | Aggregator owns the panel state machine | decided | States defined here are set by the aggregator and drawn by the renderer |
| [[HLIN-A-0003]] | Explicit envelopes, decoupled from kinds | decided | Acceptance matrix above; `kind` is a default; envelope versions permanent |
| [[HLIN-A-0004]] | Auth hoisted, identity forwarded | decided | `forbidden` state; per-principal options |

## Open Items

- Where the view registry (kinds + acceptance matrix) lives: design system or companion crate. Pending ADR; this specification is written to be indifferent.
- Default staleness window and stream-loss grace interval: stream-protocol specification.
- Whether `select` options should accept `time_range` (entities that exist only within a window): defer until a panel needs it.
- Client-side downsampling for `timeseries` when a platform ignores `step`: renderer concern, not a contract concern.
