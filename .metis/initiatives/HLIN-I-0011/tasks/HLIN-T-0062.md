---
id: a-manifest-may-declare-modules-and
level: task
title: "A manifest may declare modules, and where the shell may load and carry for them"
short_code: "HLIN-T-0062"
created_at: 2026-09-25T00:00:58.847492+00:00
updated_at: 2026-09-25T00:02:32.450595+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# A manifest may declare modules, and where the shell may load and carry for them

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 2 of [[HLIN-I-0011]]. The manifest learns the fields [[HLIN-S-0007]]
relies on (its *Manifest fields this relies on* section), and [[HLIN-S-0001]]
is amended to say so.

## Acceptance Criteria

## Acceptance Criteria

- [x] `Manifest` gains optional `assets` (one prefix) and `routes`
      (`read` and `write`, each a list of prefixes)
- [x] `Panel` and `NavigationEntry` gain optional `ui`:
      `{ entry, bridge }`, where `bridge` is the bridge major
- [x] A panel may declare `ui`, `data` + `kind`, or both. A panel with neither
      is refused (the panel, not the manifest). `kind`/`envelope`/`data`
      become optional only where `ui` is present
- [x] Validation: prefixes are relative, segment-shaped (`/ui/`), with no
      scheme, host, `..`, empty segment, backslash or percent-encoded
      separator; `ui.entry` falls under `assets`; `ui` without `assets` is
      refused; an unsupported `bridge` major is refused with the supported set
      named
- [x] Contract: `assets`, `routes`, `ui.entry` and `ui.bridge` are in the
      contract hash. Removing or narrowing a prefix, removing `ui`, or
      changing `bridge` is breaking; adding them is not. The diff says which
- [x] Canonical form and round-trip tests; the existing reference manifest
      still validates unchanged
- [x] [[HLIN-S-0001]] amended with the new fields, their validation and their
      contract rules
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- `crates/hlin-manifest/src/manifest.rs`, `validate.rs`, `canonical.rs`,
  `diff.rs`, `path.rs` (path rules may already live here), and their tests.
- The shell's registry and catalog must keep compiling and keep offering
  data panels exactly as today. Exposing `ui` in `/api/platforms` and the
  catalog is fine to add here; mounting is [[HLIN-T-0066]].
- Supported bridge majors: `[1]`, as a constant the shell and SDK share.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 — done

- `Manifest.assets: Option<String>`, `Manifest.routes: Option<Routes>`
  (`read`, `write`, unknown fields kept), `ModuleUi { entry, bridge }` on
  `Panel.ui` and `NavigationEntry.ui`. `SUPPORTED_BRIDGE_MAJORS = &[1]` lives
  in `manifest.rs` so the shell and SDK share it. `Access` (Read/Write) names
  which list a prefix is in.
- `Panel.kind`/`envelope`/`data` are now `Option<String>`, with
  `Panel::drawn_by_shell() -> Option<DataDecl>` for the shell's side. The JSON
  is additive (a data-only manifest reads, validates, writes back and hashes
  exactly as before; the spec example's hash is pinned in a test), but the
  **Rust API change is breaking** for anyone building `Panel` by hand. Chosen
  over an empty-string sentinel because the compiler then makes every shell
  path say what it does with a module-only panel.
- `path.rs`: `check_prefix`, `check_file`, `falls_under` and `PrefixDefect`.
  Prefixes must be rooted and end in `/`; `/` alone is refused
  (`WholePlatform`); `.` segments, `?`/`#` and encoded `.` are refused too,
  beyond what the task listed.
- Validation: `PanelDefect::{NothingToDraw, MissingField, Module}`;
  `ModuleDefect::{NoAssets, InvalidEntry, EntryOutsideAssets,
  UnsupportedBridge}`. **Decision:** a bad module refuses the *module*
  (`PanelOutcome.module`). The panel is refused only when it has no data to
  fall back on, which matches HLIN-S-0007's fallback for a malformed module.
  Bad `assets`/`routes` prefixes are informational (`unusable_assets`,
  `unusable_routes`), like `events`. Navigation modules are reported in
  `rejected_navigation_modules`, and the entry stays a link.
- Contract: `assets`, `routes` (sorted, deduplicated), panel `ui` and
  navigation `ui` (keyed by `path`) enter the content only when declared.
  Diff: `AssetsChanged` (additive when widened), `RoutePrefixAdded/Removed`
  (a removal is excused when a remaining prefix covers it),
  `ModuleAdded/Removed/EntryChanged`, `BridgeChanged` via `ModuleSite`,
  `DataAdded/Removed`. **Decision:** moving `ui.entry` is breaking, for the
  same reason moving `data` is.
- Shell: `accepted_panels()` offers only panels the shell can draw, so
  module-only panels stay hidden until [[HLIN-T-0066]]. `catalog_panel`
  returns `Option`. `ui` is **not** exposed in `/api/platforms` or the catalog
  yet (left for [[HLIN-T-0066]]).
- [[HLIN-S-0001]] amended: new fields, a *Modules* section (prefix rules,
  validation), contract content and diff rows, degradation rows, a REQ-1.4
  note and a decision-log row.

Tests: `crates/hlin-manifest/tests/modules.rs` (29 tests, with fixture
`module-example.json`), prefix unit tests in `path.rs`, and a registry test
for the offering rule. `angreal check all` and `angreal test all` pass.

Noticed: HLIN-S-0007's example `select` has no `label`, which `select`
requires. The fixture adds one.
