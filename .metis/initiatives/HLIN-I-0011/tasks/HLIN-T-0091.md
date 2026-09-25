---
id: compress-each-served-file-once-not
level: task
title: "Compress each served file once, not on every request"
short_code: "HLIN-T-0091"
created_at: 2026-09-25T18:44:43.678176+00:00
updated_at: 2026-09-25T21:22:45.837247+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Compress each served file once, not on every request

## Parent Initiative

[[HLIN-I-0011]]

## Objective

[[HLIN-T-0086]] made the shell compress what it serves, which cut "Twenty"'s
cold first screen from 7.73 MB to 2.27 MB and first content over a 50 Mbit/s
link from 1.75 s to 1.37 s. But it compresses on every request, and on
loopback cold first content got slower, 588 to 817 ms: brotli on the
frontend's wasm takes 223 ms in the demo's debug shell (22 ms optimised).
Module files are content-addressed and mostly `immutable`; compressing each
one once and keeping the result costs memory, not time per request.

## Acceptance Criteria

## Acceptance Criteria

- [ ] Compressed bodies are kept, keyed by the file's content (ETag or hash)
      and encoding, bounded in memory, and reused; a changed file is
      compressed afresh
- [ ] The shell's own frontend is compressed once at start, or on first
      request and kept
- [ ] Cold first content on loopback is back at or under the uncompressed
      figure, with the wire savings kept; medians of 3 recorded in
      [[HLIN-I-0012]]'s Results
- [ ] `/p/` still never compressed
- [ ] `angreal check all`, `angreal test all`, and the twenty suites pass

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0086]]'s caveat. Not started.
