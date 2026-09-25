---
id: don-t-end-a-well-behaved-stream-as
level: task
title: "Don't end a well-behaved stream as too fast because its bytes arrived in a burst"
short_code: "HLIN-T-0089"
created_at: 2026-09-25T17:08:43.769412+00:00
updated_at: 2026-09-25T17:09:22.117906+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Don't end a well-behaved stream as too fast because its bytes arrived in a burst

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0071]]. `e2e/tests/streams.spec.js` "stops pulling" fails
when the new containment specs run before it, and passes alone. The asset
proxy and the request proxy share the shell's `proxying` client, so large
module downloads leave a warm pooled connection to the platform. A module
that pauses pulling lets the platform's data back up in socket buffers; when
it pulls again, the shell reads the backlog in one burst, and the
`stream_bytes_per_second` bucket ends a stream that was never too fast as
`rate`.

The limit exists to stop a platform flooding the page. It should measure the
platform's pace, or what is delivered to the module, not how fast the shell
drains a backlog that the module's own pause created.

## Acceptance Criteria

## Acceptance Criteria

- [ ] The cause confirmed, and the rate measured so that a backlog released
      by the module's own credit does not count as the platform sending fast
      (e.g. account for bytes as they are delivered against credit, or give
      streams their own unpooled connection, or both)
- [ ] A platform that really does send faster than the limit is still ended
      with `rate`
- [ ] `streams.spec.js` passes in the full standard suite, in any order,
      three runs in a row
- [ ] `angreal check all`, `angreal test all`, and the standard suite pass

## Implementation Notes

- `crates/hlin/src/modules/requests.rs` (the streamed passthrough and its
  rate bucket), `crates/hlin/src/clients.rs` (`proxying`).

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0071]]'s finding. The standard browser suite has this
one known failure on main until it is fixed.
