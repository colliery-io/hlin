---
id: carry-a-module-s-requests-to-its
level: task
title: "Carry a module's requests to its own platform, as the viewer"
short_code: "HLIN-T-0065"
created_at: 2026-09-25T00:01:02.635990+00:00
updated_at: 2026-09-25T00:38:44.251766+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


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

## Acceptance Criteria

## Acceptance Criteria

- [x] `/p/{platform}/{path}` for `GET HEAD POST PUT PATCH DELETE`, following
      steps 1–8 of the spec in order: caller (`Sec-Fetch-Site: same-origin`
      and the shell's `Origin`), person, path, method against `routes.read` /
      `routes.write`, write rules (`Author`, `no_identity` for collapsing
      strategies, `Idempotency-Key` required), identity (bound token on
      writes from [[HLIN-T-0063]]), call, answer
- [x] Only the allowed request and response headers cross, as specified
- [x] Bodies bounded by `request_bytes` / `response_bytes`
- [x] Every shell refusal carries `X-Hlin-Refusal: {code}` and the specified
      status; platform answers pass through unchanged
- [x] A 401 from a platform is logged on the operator channel
- [x] Nothing is retried
- [x] Integration tests against a fake platform for every step and every
      refusal code, including a read-only shell and a `static-bearer`
      platform
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0062]] (routes) and [[HLIN-T-0063]] (bound tokens).
- Reuse the credentialers in `crates/hlin/src/identity.rs` and the `Author`
  extractor; `bounded.rs` for bodies.
- Streaming passthrough is [[HLIN-T-0068]]; leave a seam for it.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-25

Implemented in `crates/hlin/src/modules/requests.rs`, routed as
`/p/{platform_id}/{*path}` with `any`, so every method reaches the handler
and gets the shell's own refusal. Tested in `crates/hlin/tests/requests.rs`
(37 tests against a real loopback platform, every step and refusal code the
server can produce; `too_many` is the page's and has no server path).

Decisions the task and specification left open:

- **`Origin` on reads.** Browsers send no `Origin` on a same-origin `GET` or
  `HEAD`, so step 1 requires `Sec-Fetch-Site: same-origin` always, an
  `Origin` equal to the shell's whenever one is sent, and an `Origin` on every
  write. Requiring it on reads, as the spec reads literally, would refuse
  every read the page makes. The spec should say this.
- **Credentialer seam.** `Credentialer` gained `acts_as_viewer()` (default
  false, so a new strategy carries no writes until someone decides it should)
  and `write_headers(viewer, &BoundRequest)`. `hlin-token` mints a bound token;
  `forward-session` acts and forwards the same cookies, because the platform
  shares the session and already accepts it for writes from its own pages;
  `static-bearer` and `none` answer `no_identity`. `collapses_principals` is
  unchanged, since `none` does not collapse for deduplication.
- **Path.** Checked with `hlin_identity::normalise_path` (the rules a platform
  applies to `htu`), plus a `:` in the first segment. Sent upstream as the
  normal form re-encoded once, so the path matched, the path bound and the path
  the platform checks are one. Read from the raw URI, not axum's decoded
  wildcard.
- **Method vs path.** Step 3 matches against the prefixes for the method's
  kind, so a read under a write-only prefix is `outside_prefix` (404), not
  `method`. A method that is neither must still fall under some prefix, then
  step 4 answers `method`; an odd method on an undeclared path is 404.
- **Bodies.** A write's body is read and bounded before minting (step 6), not
  during the call, so a slowly sent body cannot spend the 30-second token.
  Reads send no body. The page going away mid-body answers `unreachable`.
- **Viewer with no credential** (a `forward-session` viewer without the
  cookie) answers `no_identity` 409 on reads and writes.
- **Redirects.** A third client, `Clients::proxying` / `AppState::proxy_client`,
  with the upstream timeout and trust anchors and `redirect::Policy::none()`,
  named for both `/p/` and `/m/`. A platform's 3xx passes back as its answer
  without `Location` (not in the allowlist): a `fetch` from the page would
  otherwise follow it with the viewer's session. Tested against a second
  server that must receive nothing.
- **Seam for [[HLIN-T-0068]].** Step 8 calls `deliver_whole`; streaming
  branches there.
- `X-Hlin-Instance` is accepted and not forwarded; nothing server-side uses it
  yet.
