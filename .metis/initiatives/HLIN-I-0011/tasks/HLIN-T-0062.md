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

- [ ] `Manifest` gains optional `assets` (one prefix) and `routes`
      (`read` and `write`, each a list of prefixes)
- [ ] `Panel` and `NavigationEntry` gain optional `ui`:
      `{ entry, bridge }`, where `bridge` is the bridge major
- [ ] A panel may declare `ui`, `data` + `kind`, or both. A panel with neither
      is refused (the panel, not the manifest). `kind`/`envelope`/`data`
      become optional only where `ui` is present
- [ ] Validation: prefixes are relative, segment-shaped (`/ui/`), with no
      scheme, host, `..`, empty segment, backslash or percent-encoded
      separator; `ui.entry` falls under `assets`; `ui` without `assets` is
      refused; an unsupported `bridge` major is refused with the supported set
      named
- [ ] Contract: `assets`, `routes`, `ui.entry` and `ui.bridge` are in the
      contract hash. Removing or narrowing a prefix, removing `ui`, or
      changing `bridge` is breaking; adding them is not. The diff says which
- [ ] Canonical form and round-trip tests; the existing reference manifest
      still validates unchanged
- [ ] [[HLIN-S-0001]] amended with the new fields, their validation and their
      contract rules
- [ ] `angreal check all`, `angreal test all` pass

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
