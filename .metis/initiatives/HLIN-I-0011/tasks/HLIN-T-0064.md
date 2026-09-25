---
id: serve-module-assets-from-the-shell
level: task
title: "Serve module assets from the shell's origin, confined by their own CSP"
short_code: "HLIN-T-0064"
created_at: 2026-09-25T00:01:01.398244+00:00
updated_at: 2026-09-25T00:23:24.637588+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Serve module assets from the shell's origin, confined by their own CSP

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 4 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Assets*, *The module CSP* and
*The shell page's CSP*.

## Acceptance Criteria

## Acceptance Criteria

- [ ] `GET /m/{platform}/{path}` fetches `{base}{path}` from the platform as
      the shell itself (no viewer identity), only when `path` falls under the
      platform's `assets` prefix by segment. Everything else is 404, including
      `..`, encoded separators and unknown platforms
- [ ] Sizes bounded by `entry_bytes` and `asset_bytes`, read with the existing
      bounded reader
- [ ] `.wasm` served as `application/wasm`, `.html` as `text/html`; other
      types from the platform
- [ ] Caching: the entry revalidated on every mount; other assets cached when
      the platform marks them `immutable`, revalidated otherwise
- [ ] Every `/m/` response carries the module CSP exactly as specified, with
      the shell's explicit origin; the shell page carries
      `frame-src {shell}/m/`
- [ ] `[modules.limits]` configuration with per-platform overrides and the
      specified defaults, checked at startup (zero or nonsense refused with the
      platform named). Shared with [[HLIN-T-0065]]; whichever lands first adds
      it
- [ ] Integration tests against a fake platform for each rule above
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0062]] for `assets`.
- New module in `crates/hlin/src/` (e.g. `modules/assets.rs`), routed in
  `server.rs`; limits in `config.rs`.
- The shell's origin for CSP comes from configuration (`public_url` where
  `oidc` sets one; otherwise a new setting). Decide and document it.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
