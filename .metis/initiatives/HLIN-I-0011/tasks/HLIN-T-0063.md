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

- [ ] `hlin-identity` mints a bound token: same claims as today plus `htm`,
      `htu`, and a 30-second `exp`
- [ ] `Verifier` gains a request-aware check that, given method and path,
      accepts an unbound token only for `GET`/`HEAD`, and for any other method
      requires `htm` and `htu` to match exactly
- [ ] Path comparison is on a normalised form both sides agree on (percent
      decoding once, no trailing-slash games); tests for the look-alikes
- [ ] A platform-side helper (an axum extractor or function, matching how the
      sample platform verifies today) that refuses an unbound or mismatched
      token on a write with 401 and says why
- [ ] Reads stay unbound and unchanged; every existing identity test passes
- [ ] [[HLIN-S-0004]] amended: the claims, the rule, the lifetime, and the
      open question about signing the path closed for writes
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- `crates/hlin-identity/src/claims.rs`, `issuer.rs`, `verifier.rs`,
  `extract.rs`; tests in `crates/hlin-identity/tests/`.
- The shell does not call this yet; [[HLIN-T-0065]] does.
- `hlin-identity` is published: keep the change additive (new functions,
  optional claims) so 0.0.x consumers still build.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
