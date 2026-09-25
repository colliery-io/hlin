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


exit_criteria_met: true
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

- [x] The cause confirmed, and the rate measured so that a backlog released
      by the module's own credit does not count as the platform sending fast
      (e.g. account for bytes as they are delivered against credit, or give
      streams their own unpooled connection, or both)
- [x] A platform that really does send faster than the limit is still ended
      with `rate`
- [x] `streams.spec.js` passes in the full standard suite, in any order,
      three runs in a row
- [x] `angreal check all`, `angreal test all`, and the standard suite pass

## Implementation Notes

- `crates/hlin/src/modules/requests.rs` (the streamed passthrough and its
  rate bucket), `crates/hlin/src/clients.rs` (`proxying`).

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0071]]'s finding. The standard browser suite has this
one known failure on main until it is fixed.

### 2026-09-25 — the rate measures the platform

**Cause, confirmed.** With the shell logging every piece of the held stream
in a failing standard run (aurora): the feed sent about 315 KB/s; the shell
read 4 MB into the browser's buffers, the stream was held for 7.9 s, and on
the first pull the shell read about 1 MiB of backlog in 8 ms, emptying the
one-second bucket: ended `rate`. The warm pooled connection is what lets that
much back up; the bucket counting the drain as the platform's pace is the
bug.

**Fix** (`crates/hlin/src/modules/requests.rs`). The bucket measures the
platform's pace. The time between the page taking a frame and asking for the
next is the page holding the stream, and earns its allowance past the
one-second cap, since a platform within its rate cannot have sent more than
that meanwhile. Once the shell waits 10 ms or more for the platform's next
piece (`CAUGHT_UP`; a backlog is handed over in microseconds) the backlog is
gone and the saving drops back to a second's allowance. A platform over its
rate still runs out: never more than the rate for the stream's age and a
second's allowance. `clients.rs` is unchanged: separate unpooled connections
were not needed once the measure was right. Spec *Streaming* and *Limits*
amended; the e2e test's comments no longer describe the old trade-off.

**Tests.** Unit: a hold keeps the allowance for its backlog; catching up
drops it; a hold does not let a flood through. Integration
(`crates/hlin/tests/requests.rs`): a platform at half `narrow`'s rate, held
3 s and then read, finishes (ends `rate` without the fix); a flood held 2 s
is still ended `rate`, with at most 4 KiB through.

**Checks.** `angreal check all` clean; `angreal test all` 1020 passed, 0
failed, 3 ignored; the standard suite (`--with aurora`) three runs in a row,
each 76 passed, 0 failed, 28 skipped. Its normal order only; no other order
was tried.
