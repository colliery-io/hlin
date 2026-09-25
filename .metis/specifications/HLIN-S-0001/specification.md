---
id: manifest-schema
level: specification
title: "Manifest Schema"
short_code: "HLIN-S-0001"
created_at: 2026-08-04T14:40:59.479999+00:00
updated_at: 2026-08-04T14:40:59.479999+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Manifest Schema

## Overview

The manifest is the document each platform serves about itself: what it is, what navigation it contributes, what panels it offers, and under what contract. It is the root contract of Hlin — discovery, the registry, the aggregator, the layout engine, and the renderer all read from it, and nothing else crosses the platform/shell boundary except panel data.

This specification defines the document's format, its fields, its versioning and contract-identity rules, and the validation and degradation semantics the shell applies to it. The view-kind, envelope, and parameter vocabularies it references are bounded sets defined in a companion specification; this document defines only how the manifest names them.

## System Context

### Actors
- **Platform teams**: author and serve a manifest; changing it is a deploy of their platform and nothing else.
- **Manifest client (shell)**: fetches, parses, and validates manifests using the shell's own service identity, never a user's; classifies failures as unreachable vs. malformed.
- **Registry (shell)**: holds the sequence of manifests per platform; computes contract identity, diffs against the last-seen contract, and enforces versioning rules at runtime ([[HLIN-A-0002]]).
- **Aggregator (shell)**: consumes validated contracts, fetches panel data on a user's behalf with forwarded identity ([[HLIN-A-0004]]), and owns the resulting panel states ([[HLIN-A-0001]]).
- **Layout engine / renderer (shell)**: consume validated panel declarations; never see a raw manifest.

### Boundaries
Inside scope: the manifest document, its lifecycle, its contract identity, and the shell's response to every possible state of it. Outside scope: the shape of envelope documents, the definitions of the vocabularies, the stream protocol, and discovery transport (the platform list comes from shell runtime configuration, behind the registry trait).

Because the manifest is fetched by the shell and not by a user, it is identical for every user. Per-user visibility is decided by platforms at data-fetch time ([[HLIN-A-0004]]); the manifest is never filtered.

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | A platform serves exactly one manifest at `GET <platform base>/.well-known/hlin.json`, `application/json`. The shell fetches it with its own service identity; the manifest is identical for all users | One well-known place; nothing to register beyond the platform's base URL in shell config; visibility is a data-fetch concern ([[HLIN-A-0004]]) |
| REQ-1.2 | Every path in a manifest (`data`, `health`, `navigation[].path`, parameter option endpoints) is resolved by appending it to the platform base the manifest was fetched from, after stripping any leading `/`. Absolute URLs, scheme-relative URLs, and `..` segments are malformed | Platforms live behind path-prefixed routing; ordinary URL resolution of `/api/...` would escape the prefix. A manifest can never point the shell outside its own base |
| REQ-1.3 | Unknown fields at any level are ignored, never rejected | Additive evolution without coordinated upgrades (vision) |
| REQ-1.4 | Every icon and view-kind reference is a name from a shared vocabulary; the manifest carries no code, markup, styles, or URLs-as-rendering. Amended 2026-09-24: a manifest may *point at* a UI module (`ui`, under a declared `assets` prefix), which the shell hosts in a sandboxed frame ([[HLIN-A-0014]]). The manifest still carries no code itself, and nothing in it is rendered as markup by the shell | The boundary carries data only (vision); modules cross it only inside the frame [[HLIN-S-0007]] specifies |
| REQ-1.5 | `platform.id` must equal the id the shell's runtime configuration assigns to the base it fetched from; a mismatch is malformed | Shell config is the single authority on platform identity; two manifests cannot claim one id |
| REQ-2.1 | The shell supports manifests at its current `schema_version` and all lower versions | A platform never has to upgrade to stay renderable |
| REQ-2.2 | The registry computes the contract hash itself from the fetched manifest; no platform-declared hash exists in the schema | Self-verifiable change detection ([[HLIN-A-0002]]) |
| REQ-2.3 | A breaking contract diff without a major `contract_version` bump is rendered exactly as delivered and raised as an operator signal; it never degrades a panel and is never a shell error | Runtime enforcement; users do not pay for a platform's versioning mistake ([[HLIN-A-0002]]) |
| REQ-2.4 | A `contract_version` lower than the last seen is a rollback: applied, logged informationally, never flagged. A new contract hash is classified only after a shell-configured number of consecutive observations (default 2) | Rollbacks and blue/green rollouts must not generate violations ([[HLIN-A-0002]]) |
| REQ-3.1 | Every possible manifest state (absent, unreachable, malformed, valid-with-invalid-panels, valid) has a defined shell behavior specified in this document | Rendering is total over its inputs (vision) |
| REQ-3.2 | Validation isolates panels: a defect in one panel declaration rejects that panel only. Platform-level malformed is reserved for defects that make the document unusable as a whole | Maximum salvage; one typo must not take down a platform's other panels |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | A manifest is parseable and validatable without any other platform's manifest, and without shell state other than the vocabularies | Platforms are independent; validation is local |
| NFR-1.2 | Canonicalization and hashing are specified precisely enough that independent implementations produce identical hashes | The hash is a contract identity, not a cache key |
| NFR-1.3 | The reference Rust types in `hlin-manifest` round-trip serde serialization for every example in this specification and preserve unknown fields on re-serialization | The crate is the spec's executable form |

## The Document

JSON object, top level:

| Field | Type | Required | Contract content | Description |
|---|---|---|---|---|
| `schema_version` | integer ≥ 1 | yes | no | Version of this document format. Monotonic, additive, shell-owned axis |
| `contract_version` | string (semver) | yes | no | The platform's declared panel-contract version; majors declare breakage ([[HLIN-A-0002]]) |
| `platform` | object | yes | no | Identity block |
| `navigation` | array | no (default `[]`) | no | Navigation entries this platform contributes |
| `panels` | array | no (default `[]`) | **yes** | Panel declarations — the contract |
| `health` | string (path) | yes | no | Health endpoint |
| `events` | string (path) | no | **no** | SSE endpoint on which this platform reports that a panel changed. Added 2026-09-08; see [[HLIN-S-0006]]. Not contract, because the shell is correct without it by construction ([[HLIN-A-0011]]) |
| `assets` | string (prefix) | no; required for any `ui` to be hostable | **yes** | The one prefix under which this platform's module code lives, e.g. `/ui/`. The shell serves module assets from its own origin, fetched from under this prefix only ([[HLIN-S-0007]] *Assets*). Added 2026-09-24 |
| `routes` | object: `read` and `write`, each an array of prefixes (default `[]`) | no | **yes** | The platform routes its modules may call through the shell's request proxy: reads (`GET`, `HEAD`) under a `read` prefix, writes (`POST`, `PUT`, `PATCH`, `DELETE`) under a `write` prefix ([[HLIN-S-0007]] *The request proxy*). One set per platform. Added 2026-09-24 |

There is no `summary` endpoint. At-a-glance status is an ordinary panel with a declared envelope; an undeclared shape crossing the boundary would contradict REQ-1.4.

### `platform`

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string, `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$` | yes | Must match the id assigned in shell configuration for this base (REQ-1.5). Layout references and dedup key off it; the manifest restates it so the document is self-describing, but config is authoritative |
| `name` | string | yes | Human-readable display name |
| `icon` | string (icon vocabulary name) | no | Fallback per vocabulary rules if absent or unknown |

### `navigation[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `label` | string | yes | Display label |
| `path` | string (relative path) | yes | Target within the platform's own frontend |
| `icon` | string (icon vocabulary name) | no | |
| `weight` | integer | no (default 0) | Sort hint within the platform's group; shell owns overall nav ordering |
| `ui` | module declaration (see *Modules*) | no | A module the shell hosts as this entry's page. The entry keeps its `path`, which stays the link when the module cannot be hosted. Added 2026-09-24 |

### `panels[]`

| Field | Type | Required | Contract content | Description |
|---|---|---|---|---|
| `key` | string, same pattern as `platform.id`, unique within the manifest | yes | yes | Stable panel identity. Layouts reference `platform.id/key`. Removal is breaking |
| `title` | string | yes | no | Default title; users override per layout |
| `description` | string | no | no | Shown in the panel picker |
| `ui` | module declaration (see *Modules*) | no | **yes** | The platform's own module for this panel. Added 2026-09-24 |
| `kind` | string (view-kind vocabulary name) | yes, unless `ui` is present | **no** | The *default* rendering; users may switch a panel to any kind that accepts its envelope, so `kind` is not something consumers pin to ([[HLIN-A-0003]] refinement). Unknown kind → fallback rendering, never a broken layout |
| `envelope` | string (envelope vocabulary name, versioned, e.g. `series.v1`) | yes, unless `ui` is present | yes | What the data endpoint returns. Named explicitly, decoupled from `kind` ([[HLIN-A-0003]]). Envelope versions are permanent |
| `data` | string (relative path) | yes, unless `ui` is present | yes | Endpoint returning one envelope document of the declared type |

A panel is drawn by the shell from `kind`, `envelope` and `data`, by its platform's module from `ui`, or offers both. `kind`, `envelope` and `data` are declared together or not at all; they may be omitted only when `ui` is present. A panel declaring both is drawn by its module and falls back to the shell's drawing when the module is unavailable ([[HLIN-S-0007]] *Fallback*). A panel declaring neither is rejected as malformed (the panel, not the manifest).
| `params` | array of parameter declarations | no (default `[]`) | yes | Shell-level controls this panel responds to. See below |
| `component` | string | no | **no** | A component in the design system, by whatever name that design system uses. Added 2026-09-08, recording what shipped with component forwarding. Never interpreted by the shell: it is forwarded to whichever design pack is mounted, and a pack that does not recognise it draws the declared `kind` instead |
| `refresh_ms` | integer ≥ 1 | no | **no** | How often this panel's data is worth refetching. Added 2026-09-08, recording what [[HLIN-A-0009]] shipped. A hint the shell clamps to its own floor, not an instruction: the publisher knows their data's cadence and the shell answers for the load |
| `lifecycle` | object | no (default `{"status": "active"}`) | yes | See below |
| `pushed` | boolean | no (default `false`) | **no** | That this platform reports changes to this panel on its `events` stream. Added 2026-09-08; see [[HLIN-S-0006]]. Required as a *separate* declaration from `events` because without it the shell cannot tell "no event yet" from "never reported on", and would relax the polling of a panel it will never hear about |

### `panels[].params[]`

A parameter declaration names a parameter from the shared vocabulary and supplies whatever configuration that vocabulary entry defines. Configuration is data; the manifest never declares a parameter's type or its rendering.

Two forms are accepted:

- **String shorthand**, for parameters that take no configuration: `"time_range"`.
- **Object form**: `{ "param": "<vocabulary name>", ...configuration fields defined by that vocabulary entry }`.

The vocabulary entry defines which configuration fields exist, which are required, and how the parameter's value is encoded onto the `data` request. Unknown configuration fields are ignored. A declaration naming an unknown parameter, or missing a required configuration field, rejects that panel as malformed.

The initial vocabulary is fixed by the companion specification, but it must include at least:

- `time_range` (no configuration): the shell's global time picker.
- `select` (configuration: `id`, `label`, `options` path, optional `multiple`): a panel-scoped chooser whose options are fetched from the platform's `options` endpoint as an `options.v1` envelope. This is how a panel is parameterized by cluster, tenant, region, or any other entity, without dynamic panel keys.

Illustrative:

```json
"params": [
  "time_range",
  { "param": "select", "id": "cluster", "label": "Cluster", "options": "api/hlin/clusters" }
]
```

### `panels[].lifecycle`

Either `{"status": "active"}` or:

```json
{
  "status": "deprecated",
  "sunset": "2026-12-01",
  "successor": "new-panel-key"
}
```

`sunset` (RFC 3339 date, required when deprecated) is the end of the deprecation window; `successor` (panel key in the same manifest, optional) names the replacement. Deprecated panels render normally with a deprecation indicator; past `sunset` they become unavailable (deprecated). Deprecating is additive; removing before sunset, or removing while never having deprecated, is breaking.

### Modules

Added 2026-09-24 for [[HLIN-S-0007]] ([[HLIN-A-0014]]). A platform may ship its own UI for a panel or a navigation entry; the shell hosts it in a sandboxed frame and speaks to it over the bridge. The manifest says where the code is, which routes it may call and which bridge it speaks. It carries no code.

A **module declaration** (`ui`) is an object:

| Field | Type | Required | Description |
|---|---|---|---|
| `entry` | string (file path) | yes | The module's entry document, e.g. `/ui/items/index.html`. Must fall under `assets` |
| `bridge` | integer | yes | The major version of the bridge the module speaks. The shell supports a set of majors (currently `[1]`, a constant in `hlin-manifest` the shell and SDK share) |

**Prefixes** (`assets`, every `routes` entry) are matched against request paths by segment, so they are held to a stricter rule than other paths (REQ-1.2 still governs those). A prefix:

- begins and ends with `/` and names at least one segment: `/ui/`, `/api/v1/`. `/` alone is refused, since it would expose the platform's whole surface, including its manifest and health endpoint;
- matches by segment: `/ui/` covers `/ui/items/app.wasm` and never `/uix/app.wasm`;
- carries no scheme, no host, no empty segment (so no `//`), no `.` or `..` segment, no backslash, no percent-encoded `/`, `\` or `.`, and no `?` or `#`.

`ui.entry` is held to the same rule, except that it names a file, so it must not end with `/`, and it must fall under `assets`.

**Validation.** A module problem costs the module, and the rest of the document stands:

| Defect | Outcome |
|---|---|
| `assets` or a `routes` prefix unusable | Informational, like an unusable `events` path: the prefix is ignored and reported to operators. Never a document defect |
| `ui` on a platform without a usable `assets` prefix; `ui.entry` unusable or not under `assets`; `ui.bridge` not a supported major (the refusal names the supported set) | The module is refused. A panel that also declares data is still accepted and drawn by the shell (the fallback [[HLIN-S-0007]] promises for a malformed module). A panel drawn only by its module is rejected as malformed. A navigation entry stays, as a link to its `path` |

## Contract Identity

The **contract content** of a manifest is the `panels` array reduced to its contract fields (marked above): `key`, `envelope`, `data`, `params`, `lifecycle`. `kind` is a default, not contract. Titles, descriptions, navigation, and the health path are not contract: no consumer pins to them, and changing them is never breaking. Neither are `refresh_ms`, `component`, `pushed` or the platform-level `events`, for the same reason stated four ways: each is something a platform *suggests* or *offers*, and the shell is correct when it is absent, ignored, or withdrawn. A platform that discovers its data is slower than it thought, or that adds an event stream, does not owe anybody a major version.

Modules add contract content (amended 2026-09-24): the platform's `assets` prefix and `routes`, each panel's `ui` (`entry` and `bridge`), and each navigation entry's `ui`, identified by the entry's `path` (navigation itself stays non-contract). A layout holding a module relies on its code being reachable, its routes being callable and its bridge being spoken. Each enters the content **only when declared**, so a manifest that declares no modules has exactly the contract hash it had before this amendment.

The **contract hash** is SHA-256 over the RFC 8785 (JCS) canonicalization of:

```json
{
  "panels": [ /* contract-reduced panels, sorted by key */ ],
  "assets": "/ui/",                                   /* only when declared */
  "routes": { "read": [ ... ], "write": [ ... ] },    /* only when declared */
  "navigation": [ { "path": "...", "ui": { ... } } ]  /* only entries with ui, when any */
}
```

with each `params` entry normalized to object form, `params` sorted by the JCS serialization of each entry, absent optional fields materialized to their defaults, and unknown fields excluded. A contract-reduced panel is `key`, `envelope` and `data` (when declared), `ui` as `{ "entry", "bridge" }` (when declared), `params` and `lifecycle`. `routes` has both lists materialized, each sorted and de-duplicated. Navigation modules are sorted by the JCS serialization of each entry. `contract_version` is not hash input: bumping the version without changing content is not a contract change.

### Diff classification (computed by the registry per [[HLIN-A-0002]])

| Change | Class |
|---|---|
| Add a panel; add a param to a panel; deprecate a panel (with sunset); extend a lifecycle window | Additive |
| Change `kind`, title, description, navigation, or the health path | Non-contract |
| Remove a panel key; change a panel's `envelope` or `data`; remove a param or change a param's configuration in a way its vocabulary entry defines as narrowing; shorten a sunset; remove a panel before its sunset | Breaking — a major bump is expected |
| Declare `assets` where there was none, or widen it (the new prefix covers the old); add a `routes` prefix, or replace one with a prefix that covers it; add a `ui` to a panel or navigation entry; add `kind`/`envelope`/`data` to a panel drawn only by its module | Additive |
| Withdraw, narrow or move `assets`; remove a `routes` prefix that no remaining prefix covers; remove a `ui`; change a `ui.entry` (as changing `data` is); change a `ui.bridge`; remove `kind`/`envelope`/`data` from a panel that has a `ui` | Breaking — a major bump is expected |

The diff names which of these happened (for example, a removed route prefix names the prefix, whether reads or writes, and the prefix that still covers it, if any).

Semver is descriptive: `contract_version` states what the platform claims, the hash states what is true, and the only checked claim is that a breaking diff is accompanied by a major bump. Minor and patch are informational; an additive change with no bump is not a violation.

A breaking diff accompanied by a major bump is legitimate evolution. A breaking diff without one is a **contract violation**: the shell renders the new manifest exactly as delivered (removed panels show as unavailable/unknown in layouts that referenced them, as they would after a legitimate removal) and raises an operator signal identifying the platform, the diff, and the expected version bump. Violations never degrade panels.

A `contract_version` lower than the last seen is a **rollback**: applied and logged informationally, never flagged. A new hash is classified only after it has been observed on a shell-configured number of consecutive polls (default two), so blue/green and canary rollouts do not generate a signal per flip.

## Validation and Degradation

| Manifest state | Shell behavior |
|---|---|
| Unreachable (network failure, non-2xx, timeout) | Platform degrades to a plain nav link with a warning indicator; last-known-good contract stays in effect for existing layouts; panels go stale, then unavailable (unreachable) per aggregator policy |
| Malformed at document level (unparseable JSON; missing or invalid `schema_version`, `contract_version`, `platform`, or `health`; `platform.id` mismatch with shell config) | Same degradation as unreachable, with panels classified malformed; the specific defect is surfaced to operators |
| `schema_version` above the shell's | Treated as valid at the shell's version: known fields honored, unknown ignored; informational signal to operators |
| Valid document, but a panel declaration is defective (missing required field, pattern violation, bad path, unknown parameter, missing required parameter configuration) | That panel only is rejected as unavailable (malformed); every other panel is processed normally (REQ-3.2) |
| Valid document, duplicate panel keys | Every panel sharing the duplicated key is rejected as malformed; the shell does not guess which one was meant |
| Valid, but a panel declares neither `ui` nor `kind`/`envelope`/`data`, or only part of `kind`/`envelope`/`data` | That panel only is rejected as unavailable (malformed) |
| Valid, but a module cannot be hosted (see *Modules*) | The module is refused; the panel falls back to the shell's drawing if it declares data, and is otherwise rejected as unavailable (malformed); a navigation entry stays a link |
| Valid, but `assets` or a `routes` prefix is unusable | Informational signal to operators; that prefix is ignored |
| Valid, but a panel names an unknown `kind` | Panel renders as the fallback kind (vocabulary spec defines it); never breaks a layout |
| Valid, but a panel names an unknown `envelope`, or a kind/envelope pairing the vocabulary does not allow | Panel unavailable (malformed) ([[HLIN-A-0003]]) |
| Valid, panel deprecated past sunset, or referencing a removed platform | Panel unavailable (deprecated) / (unknown) respectively |
| Valid | Panels eligible for layouts; contract diffed per the rules above |

A manifest problem is never a shell error, and no state exists without a defined rendering. Data-fetch outcomes (including a 403 from a platform, which yields unavailable (forbidden) per [[HLIN-A-0004]]) are stream-protocol concerns, not manifest states.

## Example

```json
{
  "schema_version": 1,
  "contract_version": "2.1.0",
  "platform": { "id": "orebank", "name": "Orebank", "icon": "database" },
  "navigation": [
    { "label": "Ingest", "path": "ingest", "icon": "inbox", "weight": 10 }
  ],
  "panels": [
    {
      "key": "ingest-throughput",
      "title": "Ingest throughput",
      "description": "Records per second across ingest workers",
      "kind": "timeseries",
      "envelope": "series.v1",
      "data": "api/hlin/ingest-throughput",
      "params": [
        "time_range",
        { "param": "select", "id": "cluster", "label": "Cluster", "options": "api/hlin/clusters" }
      ],
      "lifecycle": { "status": "active" }
    },
    {
      "key": "queue-depth",
      "title": "Queue depth",
      "kind": "stat",
      "envelope": "scalar.v1",
      "data": "api/hlin/queue-depth",
      "lifecycle": { "status": "deprecated", "sunset": "2026-12-01", "successor": "queue-depth-by-stage" }
    },
    {
      "key": "queue-depth-by-stage",
      "title": "Queue depth by stage",
      "kind": "table",
      "envelope": "records.v1",
      "data": "api/hlin/queue-depth-by-stage",
      "params": ["time_range"]
    }
  ],
  "health": "api/health"
}
```

(Kind, envelope, and parameter names in the example are illustrative until the vocabulary specification fixes the initial sets. Paths carry no leading slash; they are appended to the platform base per REQ-1.2.)

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0001]] | Aggregator owns the panel state machine | decided | State travels in the stream frame, not the envelope; renderer draws what it is told, with one local transition on stream loss |
| [[HLIN-A-0002]] | Content hash + semver, enforced at runtime | decided | Hash detects change, semver declares intent; the shell is CI, the renderer is CD; violations signal, never degrade; rollbacks and flapping tolerated |
| [[HLIN-A-0003]] | Explicit envelopes, decoupled from kinds | decided | `kind` is a default; `envelope` + `data` are the contract; envelope versions are permanent |
| [[HLIN-A-0004]] | Auth hoisted to Hlin, identity forwarded, dedup per principal | decided | Manifest is identical for all users; platforms authorize at fetch time; adds unavailable (forbidden) |
| [[HLIN-A-0014]] | Platforms ship UI modules, sandboxed per frame | decided | Adds `assets`, `routes` and `ui`, all contract; a panel may be drawn by its module, the shell, or both (amended 2026-09-24, [[HLIN-S-0007]]) |

## Open Items

- Initial vocabularies (view kinds, envelopes, parameters, icons) — next specification in this initiative; the manifest only names them. Must include `time_range`, `select`, and the `options.v1` envelope.
- `time_range` and `select` encoding onto `data` requests (query parameter names, format) — belongs to each parameter vocabulary entry.
- Aggregator staleness thresholds, last-known-good retention, and the stream-loss grace interval — aggregator/stream-protocol specification.
- Shell-issued identity token format and platform-side verification library — deployment design, downstream of [[HLIN-A-0004]].
