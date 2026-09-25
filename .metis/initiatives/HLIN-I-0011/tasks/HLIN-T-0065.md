---
id: carry-a-module-s-requests-to-its
level: task
title: "Carry a module's requests to its own platform, as the viewer"
short_code: "HLIN-T-0065"
created_at: 2026-09-25T00:01:02.635990+00:00
updated_at: 2026-09-25T00:01:02.635990+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Carry a module's requests to its own platform, as the viewer

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 5 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *The request proxy* and
*Refusals*, implementing [[HLIN-A-0013]].

## Acceptance Criteria

- [ ] `/p/{platform}/{path}` for `GET HEAD POST PUT PATCH DELETE`, following
      steps 1–8 of the spec in order: caller (`Sec-Fetch-Site: same-origin`
      and the shell's `Origin`), person, path, method against `routes.read` /
      `routes.write`, write rules (`Author`, `no_identity` for collapsing
      strategies, `Idempotency-Key` required), identity (bound token on
      writes from [[HLIN-T-0063]]), call, answer
- [ ] Only the allowed request and response headers cross, as specified
- [ ] Bodies bounded by `request_bytes` / `response_bytes`
- [ ] Every shell refusal carries `X-Hlin-Refusal: {code}` and the specified
      status; platform answers pass through unchanged
- [ ] A 401 from a platform is logged on the operator channel
- [ ] Nothing is retried
- [ ] Integration tests against a fake platform for every step and every
      refusal code, including a read-only shell and a `static-bearer`
      platform
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0062]] (routes) and [[HLIN-T-0063]] (bound tokens).
- Reuse the credentialers in `crates/hlin/src/identity.rs` and the `Author`
  extractor; `bounded.rs` for bodies.
- Streaming passthrough is [[HLIN-T-0068]]; leave a seam for it.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
