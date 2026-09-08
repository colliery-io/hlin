---
id: hlin-identity-mint-publish-verify
level: task
title: "hlin-identity: mint, publish, verify"
short_code: "HLIN-T-0010"
created_at: 2026-09-07T14:04:19.374794+00:00
updated_at: 2026-09-07T15:47:52.436411+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# hlin-identity: mint, publish, verify

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Build `hlin-identity`, the executable form of [[HLIN-S-0004]]: the shell mints a signed token per request, publishes its keys, and a platform verifies without calling back. In the strategy catalogue of [[HLIN-S-0005]] this crate is the `hlin-token` credentialer; the trait it plugs into and the other strategies land in [[HLIN-T-0011]]. This is the crate every platform will depend on to check who is asking, so the quiet failure it exists to prevent, a platform that verifies the signature but forgets the audience, must be impossible to reach through its API. Then wire the sample platform to verify.

## Acceptance Criteria

## Acceptance Criteria

- [x] New crate `crates/hlin-identity` with an `Issuer` (Ed25519 key, `kid` derived from the public key, `mint`, `jwks`) and a `Verifier` (`verify(token, expected_audience) -> Result<Claims, Refusal>`)
- [x] Tokens are JWTs signed `EdDSA` with `iss`, `sub`, `aud`, `iat`, `exp` at 120s and a unique `jti`; `name`, `email` and `groups` pass through untouched, and unknown claims from a newer shell are preserved
- [x] `Verifier::verify` checks signature, `iss`, `aud` and `exp` with 60s skew in one call with no way to skip any of it; caches for an hour; refetches on an unknown `kid`, rate limited to once a minute
- [x] `IDENTITY_HEADER` is one constant; `JWKS_PATH` and `TOKEN_LIFETIME_SECONDS` likewise
- [x] A `dev-identity` feature, refused by `compile_error!` in release, with a test that runs `cargo check --release` in a subprocess and asserts both the refusal and its message
- [x] `Issuer::load_or_generate(path)`, with a test that a restart keeps the `kid` and that tokens minted before it still verify after
- [x] An axum extractor `HlinIdentity` behind an `axum` feature, answering 401 with the reason logged and nothing platform-authored in the body
- [x] `hlin-sample-platform` verifies with the real `Verifier` in `--auth token`; `--restrict-health-to` still gives a 403 on top of a valid identity
- [x] Tests: 19 in the identity crate plus 2 guard tests, covering the round trip, wrong audience, wrong issuer, wrong key, absent, malformed, missing `kid`, caching, cold fetch, rotation, refetch rate limiting, key identity, restart, `jti` uniqueness and expiry; plus a cross-crate test in the sample platform

## Implementation Notes

### Technical Approach
Use a maintained JWT crate with EdDSA support rather than assembling from `ed25519-dalek` and base64 by hand; choose in the task and record why. Keep `Verifier` free of axum so a non-axum platform can use it; the extractor is a thin adapter. The JWKS fetch needs an HTTP client; take it as a trait so tests can serve a key set in memory.

### Dependencies
None. Can start immediately and in parallel with [[HLIN-T-0009]]; the sample-platform wiring at the end needs that task landed.

### Risk Considerations
Feature-gated compile errors are easy to get subtly wrong; the test that asserts release refusal should build the crate in release mode in a subprocess rather than trust `cfg` reasoning.

## Status Updates

**2026-09-07 — complete.** `hlin-identity` in five modules: `claims` (`Principal` and `Claims`), `keys` (JWKS and the key container), `issuer` (minting and key persistence), `verifier` (the one checking path), `extract` (the axum adapter, behind a feature). 21 tests here plus a cross-crate one in the sample platform; workspace at 130; `angreal check all` clean.

**The library choice, which the task asked be recorded.** `jsonwebtoken` 11 with its `rust_crypto` backend, over hand-assembling from `ed25519-dalek` and base64. It brings the validation the specification asks for as data rather than as code I would have to write and get right: audience, issuer, expiry and skew are all `Validation` fields, so "checks everything, in one call, with no way to skip a check" is a property of how the library is called rather than a discipline in my own control flow.

One thing did have to be hand-built, and it is worth being precise about what: the library takes an EdDSA signing key as PKCS#8 DER, and `ed25519-dalek` 3.0 does not re-export the `EncodePrivateKey` trait that would produce one. Rather than hunt for a feature combination, the crate writes the RFC 8410 container directly, which is a fixed sixteen-byte prefix followed by the thirty-two-byte seed. That is a standard with exactly one encoding, verified by round-tripping a token through the library. No cryptography is hand-rolled; a byte container is.

Decisions taken during the work:

- **`kid` is derived from the public key**, not chosen. Two shells cannot collide, and a key identifier always names the key it belongs to. A test pins that the same seed gives the same `kid` and different seeds do not.
- **`verify` takes the audience as a parameter**, not from configuration, so it cannot be left unset. There is exactly one method on `Verifier` that checks anything, and a test exists whose only job is to fail if someone adds a second.
- **Every refusal is a 401.** The extractor never returns 403, because every refusal it can produce means the credential is wrong. Whether a principal may see a panel is the platform's own decision afterwards. The sample platform demonstrates both, and the code says why in both places.
- **The refusal reason is logged and not returned.** A caller learns their identity was not accepted; an operator learns which of nine reasons.
- **`JwksFetcher` is a trait.** A platform brings its own HTTP client, and tests serve a key set from memory. The sample platform's implementation shells out to `curl`, which is honest for a demo and clearly labelled as such.

**A test I wrote wrong, and what it taught.** The rotation test asserted that a key published seconds ago is picked up immediately. It failed, correctly: the unknown-`kid` refetch is rate limited to once a minute, so a rotation inside that window waits. The design is right and the test was wrong. Rotation works by publishing the new key and waiting a propagation window before signing with it, so by the time a token needs the new key every platform has had it for a day; the refetch is a safety net for a cold cache, not the mechanism. The test now pins that trade explicitly, including that the old key keeps working throughout, which is what makes a rotation safe without coordinating deploys.

**The release guard is tested, not reasoned about.** Two `#[ignore]`d tests run `cargo check` in a subprocess: one asserts a release build with `dev-identity` fails and that its message says why, one asserts a debug build succeeds. The task flagged that `cfg` conditions are easy to invert, and the failure mode is a production binary accepting unsigned requests, so trusting my own reading of the condition was not good enough. Both pass.

Note for [[HLIN-T-0011]]: `Issuer::load_or_generate` and `Issuer::jwks` are what the shell needs for its JWKS route, and `IDENTITY_HEADER` plus `mint` are the whole of the `hlin-token` credentialer.
