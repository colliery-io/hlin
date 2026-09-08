---
id: identity-forwarding-specification
level: task
title: "Identity forwarding specification"
short_code: "HLIN-T-0006"
created_at: 2026-09-07T12:13:58.695566+00:00
updated_at: 2026-09-07T13:14:24.206892+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# Identity forwarding specification

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Specify the identity-forwarding contract from [[HLIN-A-0004]]: the token the shell attaches to every data request it makes on a user's behalf, and what a platform must do to verify it. This is a contract every platform implements, so it needs the same precision as the manifest.

## Acceptance Criteria

## Acceptance Criteria

- [x] A Metis specification under this initiative defining token format, required and optional claims, and the `X-Hlin-Identity` header — [[HLIN-S-0004]]
- [x] Signing: EdDSA over Ed25519, JWKS at `/.well-known/hlin-keys.json`, four-step rotation with a propagation window, one-hour cache with `kid`-miss refetch, 60 seconds of clock skew
- [x] Platform obligations, as a five-step procedure, with the 401/403 distinction and why conflating them hides a broken deployment behind an apparent permissions problem
- [x] The `options` endpoint for `select` parameters receives the same token (REQ-2.2)
- [x] Development mode: a compile-time feature refused in release builds, so no runtime configuration can enable a bypass
- [x] A worked example token payload and a verification walkthrough
- [x] Named follow-up: a shared verification crate, with the specific quiet failure it exists to prevent

## Implementation Notes

### Technical Approach
Specification only. Choose claim names to match RFC 7519 registered claims where they exist.

### Dependencies
[[HLIN-A-0004]]. Independent of all code tasks.

### Risk Considerations
Token lifetime vs. stream lifetime: a long-lived browser stream will outlive short tokens. The spec must say the aggregator mints per request, not per stream.

## Status Updates

**2026-09-07 — complete.** [[HLIN-S-0004]] written. Specification only, as scoped.

Decisions taken while writing it:

- **EdDSA over Ed25519, and explicitly not `HS256`.** A shared secret would put a token-*minting* key on every platform, turning twelve services into twelve places the shell's identity can be forged. The specification says so rather than leaving it as an obvious-to-some omission.
- **`kid` is required.** It is what makes rotation work without coordination: a platform that meets an unrecognised `kid` refetches the key set once instead of failing. Publish-then-switch ordering plus that refetch is the whole of NFR-1.2.
- **`aud` names the target platform, and a platform must check it.** Without it, a token captured from one platform replays against every other. This is the quiet failure the verification crate exists to prevent, and it is called out as such.
- **`groups` is passed through, never interpreted.** The shell does not know what a group means and decides nothing from one; a platform using groups is using its identity provider's data with the shell as courier.
- **The development bypass is a compile-time feature refused in release builds**, rather than a configuration flag. A flag that can be set is a flag that will be set, at three in the morning, to make something work. The verification library carries a test asserting the release build refuses it.
- **401 and 403 are load-bearing and the specification argues for the distinction** rather than just stating it. A platform returning 403 for a bad token makes a broken deployment look like a permissions problem forever, and nobody investigates permissions.

On the risk this task recorded, token lifetime against stream lifetime: REQ-1.3 says a token is minted per request, never per stream or per session, and the lifetime section states the reasoning. 120 seconds, with `jti` present so a platform may keep a replay cache without needing one.

Four open items are recorded, the sharpest being whether `sub` should be the identity provider's subject or a shell-issued pseudonym. A pseudonym keeps the provider's identifiers out of twelve platforms' logs and costs a mapping table; that is probably the right trade but it is a decision, not a detail, and it wants an ADR if it goes the pseudonym way.
