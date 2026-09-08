---
id: reshape-workspace-into-hlin-hlin
level: task
title: "Reshape workspace into hlin, hlin-manifest, hlin-view"
short_code: "HLIN-T-0001"
created_at: 2026-09-07T12:13:51.832205+00:00
updated_at: 2026-09-07T12:33:24.196441+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# Reshape workspace into hlin, hlin-manifest, hlin-view

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Replace the template's `hlin-core` and `hlin-cli` crates with the three crates decided in [[HLIN-A-0005]]: `hlin` (shell binary), `hlin-manifest` (contract types, envelopes, canonicalization, hashing), and `hlin-view` (view registry and rendering). Keep every angreal task, the docs build, and version tooling working against the new layout. This task unblocks all code tasks in the initiative.

## Acceptance Criteria

## Acceptance Criteria

- [x] `Cargo.toml` workspace members are exactly `crates/hlin`, `crates/hlin-manifest`, `crates/hlin-view`; `hlin-core` and `hlin-cli` are gone
- [x] Dependency direction is `hlin` → `hlin-view` → `hlin-manifest`; `hlin-manifest` depends on no other workspace crate
- [x] Each crate's `lib.rs`/`main.rs` carries a crate-level doc comment stating its responsibility as written in [[HLIN-A-0005]]
- [x] `angreal check all`, `angreal test all`, `angreal build`, and `angreal version verify` pass
- [x] `plissken.toml` references the new crates and `angreal docs api` regenerates cleanly; `mkdocs.yml` needed no change (it names no crates). `angreal docs build` not run end to end: `mkdocs` is not installed in this environment
- [x] README's project-structure section lists the three crates with one-line responsibilities
- [x] Unified version (0.0.1) is declared consistently across all three crates

## Implementation Notes

### Technical Approach
Rename rather than recreate where the template wiring is worth keeping (test layout, `tests/` skeletons, clippy/rustfmt config). `hlin` keeps the clap entrypoint from `hlin-cli` for now; it becomes the axum/Leptos host in a later initiative. `hlin-view` cannot yet depend on the design system (no published crate); it depends on `hlin-manifest` only, with the design-system dependency and version pin added when that crate exists.

### Dependencies
None. Blocks [[HLIN-T-0002]], [[HLIN-T-0003]], [[HLIN-T-0004]], [[HLIN-T-0007]].

### Risk Considerations
The template's version-bump task may assume crate names; verify `angreal version bump` against the new layout before closing.

## Status Updates

**2026-09-07 — complete.** Workspace reshaped and verified.

- `hlin-core` → `hlin-manifest` and `hlin-cli` → `hlin` via `git mv`, preserving history and the template's test layout; `hlin-view` created new.
- Dependency direction is `hlin` → `hlin-view` → `hlin-manifest`, with `hlin-manifest` depending on no workspace crate. Confirmed by the manifests and a clean `cargo check --workspace`.
- Each crate carries a crate-level doc comment stating its responsibility per [[HLIN-A-0005]]; the shell's names its five components per this initiative's decomposition.
- Verified passing: `angreal check all` (fmt, clippy with `-D warnings`, check), `angreal test all` (7 tests), `angreal build`, `angreal version verify` (0.0.1 in sync), and the binary itself.
- `plissken.toml`, `docs/index.md`, `docs/getting-started/installation.md`, and the README crate list all name the three crates. `mkdocs.yml` names no crates and needed no change.

Decisions taken during the work, beyond the written criteria:

- **The shell gained a library target** alongside its binary. [[HLIN-T-0007]] puts the store trait in `hlin` and wants trait-level tests against it, which a binary-only crate makes awkward. `main.rs` is now a thin entrypoint over `lib.rs`.
- **`hlin-manifest`'s error type was renamed** from the template's `AppError` to `Error`, with `Malformed` and `Invalid` variants anticipating the degradation tables. The real structured reasons land in [[HLIN-T-0002]] and [[HLIN-T-0003]].
- **Generated API docs are now gitignored** (`docs/api/rust/`, `docs/api/_nav.yml`). The docs workflow runs `plissken render` itself before `mkdocs build`, so they are build artifacts rather than sources.
- Test skeletons added for `hlin` and `hlin-view` so `angreal test integration` and `angreal test functional` cover all three crates.

Known cosmetic issue, not blocking: plissken lists `hlin` twice in the generated `docs/api/_nav.yml`, because the package has both a lib and a bin target of that name. Both entries point at the same page. Setting `doc = false` on the binary does not suppress it, so nothing was added to the manifest for it. The file is generated and gitignored; worth raising upstream if it ever bothers anyone.

`angreal docs build` could not be run end to end because `mkdocs` is not installed here. `angreal docs api`, the plissken half that this task actually changed, runs clean and emits pages for all three crates.
