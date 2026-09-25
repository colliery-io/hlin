---
id: compress-each-served-file-once-not
level: task
title: "Compress each served file once, not on every request"
short_code: "HLIN-T-0091"
created_at: 2026-09-25T18:44:43.678176+00:00
updated_at: 2026-09-25T22:21:28.685795+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


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

## Acceptance Criteria

- [x] Compressed bodies are kept, keyed by the file's content (ETag or hash)
      and encoding, bounded in memory, and reused; a changed file is
      compressed afresh
- [x] The shell's own frontend is compressed once at start, or on first
      request and kept
- [x] Cold first content on loopback is back at or under the uncompressed
      figure, with the wire savings kept; medians of 3 recorded in
      [[HLIN-I-0012]]'s Results
- [x] `/p/` still never compressed
- [x] `angreal check all`, `angreal test all`, and the twenty suites pass

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0086]]'s caveat. Not started.

### 2026-09-25 — done

**What.** `crates/hlin/src/compressed.rs` replaces tower-http's compression
layer on `/m/` and the frontend fallback with a middleware over one shared
cache on `AppState`. A compressed body is kept under the SHA-256 of the
uncompressed bytes and the encoding: shared by every request and viewer,
and by two platforms serving identical bytes; a file changed under the same
name and validator is a new key. Content rather than `ETag`, because it
cannot be wrong, and a cryptographic hash so a module cannot craft a
collision with another's file. The first request for a file is answered at
brotli 4 (gzip at its default) and kept; behind it, one at a time and with
at most 64 waiting, brotli files are compressed again at 11 and replace
what was kept (whichever is smaller; 11 is not always smaller on tiny
files). The frontend is compressed at 11 in the background from the moment
the shell starts (`Compressed::precompress`); a request before that finishes
gets the quick path. Rules as before: only a `200` to a `GET`, of known
length, at least 1 KB, not already encoded, not an image, event stream or
gRPC, with `Vary: Accept-Encoding`; `/p/` is still outside the layer.

**The bound.** A new top-level `[compression] cache_bytes`, default
**64 MiB**, least recently used first out. A file larger than the whole
budget is sent as it is (compressing it on every request is what this
removes); `cache_bytes = 0` turns compression off; 1 to 1023 is refused as
a typo. What it held after `twenty-measure`: **71 files, 3.48 MB** (every
module's assets and the frontend, brotli at 11), from the shell's own log
(`compressed a file at its best … kept_bytes`). Shell resident memory
after the suite: 83–85 MB, against 130 MB before (debug build; per-request
brotli left more behind than the cache holds). Brotli 11 is briefly
memory-hungry: 106 MB resident while the frontend was being compressed.

**Quality, measured** on the frontend's 1.72 MB wasm (release; debug in
brackets, with the profile change below): brotli 4, 531 KB in 17 ms
(150 ms); brotli 9, 469 KB in 100 ms; brotli 11, 416 KB in 1.5 s (9.4 s);
gzip 6, 602 KB in 41 ms (42 ms). So 11 is a fifth fewer bytes than 4, and
affordable only once.

**Debug profile.** The demo's shell is a debug build, where unoptimised
brotli 11 takes 29 s over that wasm and SHA-256 13 ms. The workspace's dev
profile now optimises `brotli`, `alloc-no-stdlib`, `alloc-stdlib`,
`miniz_oxide`, `crc32fast` and `sha2` (29 s → 9.4 s, 400 → 150 ms, 13 →
0.8 ms). The rest is brotli's generic code, compiled in `hlin`.

**Measured.** `angreal e2e twenty-measure` against `angreal demo up --with
twenty --release`, medians of three, same machine and method as
[[HLIN-T-0086]]. All three in the same session, each on a fresh demo:
*before* is `1745814` (compressing on every request), *off* is this change
with `cache_bytes = 0` (nothing compressed), *after* is this change.

| | Off (uncompressed) | Before | After |
|---|---|---|---|
| Cold, navigation → six drawn, loopback | 592 ms | 819 ms | **599 ms** (594 once kept at 11) |
| Cold over 50 Mbit/s, 40 ms → six drawn | 1,725 ms | 1,373 ms | **1,091 ms** |
| Warm → six drawn | 537 ms | 535 ms | 534 ms |
| Cold bytes, first screen | 7.06 MB | 2.32 MB | **1.95 MB** (1.88 once kept at 11) |
| of which the shell's frontend | 1.82 MB | 0.57 MB | **0.44 MB** |
| Every module asset once, as sent | 6.31 MB | 2.05 MB | **1.83 MB** once kept at 11 |
| Warm bytes, first screen | 0.05 MB | 0.05 MB | 0.05 MB |

Loopback is back to the uncompressed figure within the runs' spread
(uncompressed 588–601 ms, after 597–610 ms on a fresh shell), and over a
real link the first screen draws 280 ms sooner than before and 630 ms
sooner than uncompressed. The first run on a fresh shell pays the quick
compression once per module file; every run after is served from what was
kept.

**Found, not caused.** On a demo that has already run `twenty-measure`
once, running it again fails "killing dice's platform … recovers by itself"
(the open page stays `stale`). The same happens on `1745814`, so it is not
this change; the first run on a fresh demo passes.

Checks: `angreal check all` clean; `angreal test all` 1046 passed (12 in
`tests/compression.rs`, six new: kept for everybody, the same bytes kept
once, a changed file compressed afresh, the frontend at its best before
anybody asks, a file over the budget sent as it is, a budget of 0 turns it
off; six unit tests in `compressed`); standard suite (`--with aurora`) 76
passed, 28 skipped, 0 failed; `angreal e2e twenty` 6 passed;
`twenty-measure` 4 passed on a fresh demo.
