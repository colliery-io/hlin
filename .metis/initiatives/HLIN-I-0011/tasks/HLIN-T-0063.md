---
id: bind-a-write-s-token-to-its-request
level: task
title: "Bind a write's token to its request"
short_code: "HLIN-T-0063"
created_at: 2026-09-25T00:00:59.997145+00:00
updated_at: 2026-09-25T00:02:33.301934+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Bind a write's token to its request

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 3 of [[HLIN-I-0011]]. Decision 6 of [[HLIN-I-0010]] and step 6 of the
request proxy in [[HLIN-S-0007]]: a write carries the usual `hlin-token` plus
`htm` (method) and `htu` (path relative to the platform's base, without its
query), with a 30-second lifetime. A platform verifying a write requires them.

## Acceptance Criteria

## Acceptance Criteria

- [x] `hlin-identity` mints a bound token: same claims as today plus `htm`,
      `htu`, and a 30-second `exp`
- [x] `Verifier` gains a request-aware check that, given method and path,
      accepts an unbound token only for `GET`/`HEAD`, and for any other method
      requires `htm` and `htu` to match exactly
- [x] Path comparison is on a normalised form both sides agree on (percent
      decoding once, no trailing-slash games); tests for the look-alikes
- [x] A platform-side helper (an axum extractor or function, matching how the
      sample platform verifies today) that refuses an unbound or mismatched
      token on a write with 401 and says why
- [x] Reads stay unbound and unchanged; every existing identity test passes
- [x] [[HLIN-S-0004]] amended: the claims, the rule, the lifetime, and the
      open question about signing the path closed for writes
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- `crates/hlin-identity/src/claims.rs`, `issuer.rs`, `verifier.rs`,
  `extract.rs`; tests in `crates/hlin-identity/tests/`.
- The shell does not call this yet; [[HLIN-T-0065]] does.
- `hlin-identity` is published: keep the change additive (new functions,
  optional claims) so 0.0.x consumers still build.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 (implemented)

Implemented in `hlin-identity`, additively; nothing outside the crate changed.

- `Issuer::mint_bound(principal, audience, &BoundRequest)` mints the usual
  claims plus `htm` and `htu`, with `BOUND_TOKEN_LIFETIME_SECONDS = 30`.
  `BoundRequest::new(method, path)` validates the method and normalises the
  path first, so an unbindable request is refused before anything is signed.
  The shell already holds an `Arc<Issuer>`, so [[HLIN-T-0065]] only has to
  call it.
- `Verifier::verify_request(token, audience, method, path)` does all of
  `verify`, then: unbound on exact `GET`/`HEAD` is accepted unchanged;
  unbound on anything else is `RequestRefusal::Unbound`; a bound token must
  match `htm` exactly and `htu` in normal form, and live no more than 30 s.
- `normalise_path`: drop query and fragment, require a leading `/`, decode
  each segment once, refuse empty segments (so trailing and double slashes),
  dot segments in any spelling, backslashes, malformed escapes, non-UTF-8, and
  escapes decoding to `/`, `\` or a control character. Case-sensitive.
- `extract::HlinRequestIdentity` (axum feature): the request-aware extractor,
  answering 401 with `{"error", "reason"}` so a platform team sees why.
- Tests in `crates/hlin-identity/tests/binding.rs`, including about thirty
  look-alike paths. Existing tests untouched and passing.
- [[HLIN-S-0004]] amended: REQ-4.1 and 4.2, the `htm`/`htu` claims, the 30 s
  lifetime, the normal form, the platform's step 4, and the open question on
  signing the path closed for writes and left open for reads.

Decisions the task left open:

- `htm`/`htu` live in `Claims::extra` behind `htm()`, `htu()` and
  `is_bound()`, not as new struct fields: `Claims` has public fields and no
  `#[non_exhaustive]`, so a new field would break any consumer building one by
  hand (this crate's own dev-identity path does). Likewise the binding errors
  are a new `RequestRefusal` enum wrapping `Refusal`, rather than new
  `Refusal` variants, so exhaustive matches downstream keep compiling. The new
  enums are `#[non_exhaustive]`.
- A bound token is held to its binding on reads too; half a binding, or a
  non-string one, is refused on any method.
- The verifier refuses a bound token whose `exp - iat` exceeds 30 s rather
  than trusting the shell's lifetime.
- `%61` and `a` are the same path under "decode once". A router matching raw
  bytes could route them differently; accepted because the shell's proxy
  decodes the same way and RFC 3986 treats them as equivalent.
- The extractor compares `uri.path()` as axum sees it (after `nest`); a
  platform behind a path-rewriting proxy should call `verify_request` itself.
- The dev-identity bypass accepts unsigned writes too, as it does reads.

Not done, by design: the sample platform has no write routes, so it is
unchanged; wiring into the request proxy is [[HLIN-T-0065]].
