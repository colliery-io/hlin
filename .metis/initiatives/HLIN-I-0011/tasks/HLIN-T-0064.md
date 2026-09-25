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

- [x] `GET /m/{platform}/{path}` fetches `{base}{path}` from the platform as
      the shell itself (no viewer identity), only when `path` falls under the
      platform's `assets` prefix by segment. Everything else is 404, including
      `..`, encoded separators and unknown platforms
- [x] Sizes bounded by `entry_bytes` and `asset_bytes`, read with the existing
      bounded reader
- [x] `.wasm` served as `application/wasm`, `.html` as `text/html`; other
      types from the platform
- [x] Caching: the entry revalidated on every mount; other assets cached when
      the platform marks them `immutable`, revalidated otherwise
- [x] Every `/m/` response carries the module CSP exactly as specified, with
      the shell's explicit origin; the shell page carries
      `frame-src {shell}/m/`
- [x] `[modules.limits]` configuration with per-platform overrides and the
      specified defaults, checked at startup (zero or nonsense refused with the
      platform named). Shared with [[HLIN-T-0065]]; whichever lands first adds
      it
- [x] Integration tests against a fake platform for each rule above
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0062]] for `assets`.
- New module in `crates/hlin/src/` (e.g. `modules/assets.rs`), routed in
  `server.rs`; limits in `config.rs`.
- The shell's origin for CSP comes from configuration (`public_url` where
  `oidc` sets one; otherwise a new setting). Decide and document it.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 (implemented)

Done in `crates/hlin/src/modules/assets.rs`, routed in `server.rs` as
`/m/{*asset}` (plus `/m` and `/m/`, so they are refused rather than answered
with the frontend's index by the fallback). Tests in
`crates/hlin/tests/assets.rs`, 20 of them, against a platform on a real port
that records what it was asked. `angreal check all` and `angreal test all`
pass.

The `[modules.limits]` criterion was met by c65fad0 (`ModuleLimits`,
`Config::module_limits`, checked at startup) and is used here unchanged.

Decisions the task left open:

- **Origin.** `Config::origin()` from c65fad0: `public_url`, else `oidc`'s,
  else `http://localhost:{port}`.
- **As itself** means as a manifest is fetched: no credential at all, no
  cookie, nothing of the viewer. The platform's credential strategy is not
  consulted. The route asks for no caller either, since a sandboxed frame's
  opaque origin sends no session; what it serves is only what the platform
  hands the shell unauthenticated.
- **Path checking** reads the raw request URI rather than axum's decoded
  `Path`, then applies `hlin_manifest::path::check_file` and `falls_under`
  (no second matcher). A platform whose manifest is document-invalid, or has
  no `assets`, serves nothing.
- **Which file is the entry** is decided by declaration: a path any panel or
  navigation entry names as `ui.entry`. That file gets `entry_bytes` and
  `no-cache`; everything else `asset_bytes`, and the platform's
  `Cache-Control` verbatim when it contains `immutable`, else `no-cache`.
  `If-None-Match`/`If-Modified-Since` are forwarded and `ETag`,
  `Last-Modified` and `304` passed back, so revalidation is cheap.
- **Types.** `.html` is served as `text/html; charset=utf-8`; every `/m/`
  response also carries `X-Content-Type-Options: nosniff`.
- **Statuses**, for the frame host (HLIN-T-0066): any 4xx is
  `unavailable (malformed)`, any 5xx `unavailable (unreachable)`.
  404 for unknown platform / outside the prefix / a bad path; the platform's
  own 4xx passed through; 413 for over a limit; 502 for unreachable, a 5xx,
  a body cut off partway, or a redirect; 504 for a timeout.
- **CSP for an unknown platform.** The module CSP names the platform, so for
  a segment that is not a configured platform (it could hold a `;`) the
  refusal carries `default-src 'none'; frame-ancestors {shell}` instead.
- **Shell page CSP.** There was none. `server::with_frontend` wraps the
  frontend fallback (`main.rs` now calls it) and adds only
  `frame-src {origin}/m/`; API routes and `/m/` are untouched by it.

Left over:

- The shared `reqwest` client follows redirects, so a redirected asset has
  been *fetched* before it is refused. Not following at all needs a client
  with `redirect::Policy::none()` on `AppState`; left alone to keep this
  change off `AppState` while the request proxy (HLIN-T-0065) is in flight.
  Worth doing there, where it matters more.
- Queries on `/m/` URLs are dropped rather than forwarded.
