---
id: compress-what-the-shell-serves-and
level: task
title: "Compress what the shell serves, and optimise module wasm"
short_code: "HLIN-T-0086"
created_at: 2026-09-25T12:37:19.946479+00:00
updated_at: 2026-09-25T17:09:22.041742+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: true
initiative_id: HLIN-I-0011
---

# Compress what the shell serves, and optimise module wasm

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0084]]: nothing the shell serves is compressed, neither
module assets under `/m/` nor its own frontend, and module builds skip
`wasm-opt`. The first screen of "Twenty" costs 7.73 MB cold; all twenty
modules are 12.53 MB, 3.98 MB gzipped. Weight is the first risk in the bet
([[HLIN-A-0014]]), and this is most of it.

## Acceptance Criteria

## Acceptance Criteria

- [x] The shell compresses (gzip, and brotli where the browser accepts it)
      its own frontend and whatever it passes through under `/m/` that the
      platform did not already compress, honouring `Accept-Encoding`, with a
      sensible minimum size and correct `Vary`
- [x] Compression does not break streaming responses under `/p/`, and is not
      applied to them
- [x] Module and frontend release builds run `wasm-opt` (Trunk's
      `data-wasm-opt`), and the size change is recorded
- [x] The twenty measurement re-run: cold bytes for the first screen and all
      twenty, before and after, recorded in [[HLIN-I-0012]]'s results
- [x] `angreal check all`, `angreal test all`, and every browser suite pass
      — *the standard suite's one known failure, `streams.spec.js` "stops
      pulling", is [[HLIN-T-0089]]'s*

## Implementation Notes

- `crates/hlin/src/modules/assets.rs`, `server.rs` (frontend serving),
  tower-http's compression layer if it fits the bounded-body reading;
  `index.html` of each module and frontend for `data-wasm-opt`.

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.

### 2026-09-25 — done

**Compression.** `modules::assets::compression()` is tower-http's
`CompressionLayer` (gzip and brotli; brotli at quality 4, tower-http's
default, since 11 takes seconds), over 1 KB, never images, gRPC or event
streams, and it adds `Vary: Accept-Encoding`. It wraps the `/m/` routes (a
sub-router merged into the shell's) and the frontend fallback in
`server::with_frontend`. `/p/` is not under it at all, rather than excluded
by a predicate. The asset proxy now passes the browser's `Accept-Encoding`
to the platform and `Content-Encoding` and `Vary` back, so a platform that
compressed its own file has it passed on as sent, and the layer leaves an
encoded answer alone. Bounded reads are unchanged: the limit is on what
crossed the wire from the platform. `crates/hlin/tests/compression.rs`
(six tests, real router and platform): gzip and br as accepted, round-tripped;
identity with `Vary` when nothing is accepted; small files left alone; a
platform's own encoding passed through and not compressed again; a 304
still a 304; the frontend compressed with its CSP; and a `/p/` read, whole
and streamed, never compressed.

**wasm-opt.** Every module and frontend `index.html` now has
`data-wasm-opt="z"` and `data-wasm-opt-params` enabling what rustc 1.96
emits by default (bulk memory, non-trapping float-to-int, sign extension,
mutable globals, reference types, multivalue). That was the real reason it
was off: without the flags binaryen validates against the MVP and refuses
the module (`memory.copy operations require bulk memory`), at version_123
and at version_133 alike. Each `Trunk.toml` pins `wasm_opt =
"version_123"`, Trunk's default and the one it downloads. Trunk runs it only
for `--release`, so debug builds and `angreal ui build` are unchanged.

`demo up --release` first asks whether wasm-opt can be had (already in
Trunk's cache, or GitHub answers); if not, it says so and builds with
`trunk build --cargo-profile release`, the same optimised Rust without the
wasm-opt pass (checked: byte-identical to `--release` before this change).
The module fingerprint includes whether wasm-opt ran, so an offline build
is redone once it can be.

What wasm-opt saves, which is little once compressed:

| | Before | `-Oz` | Change |
|---|---|---|---|
| 20 module wasm files | 11.75 MB | 10.54 MB | −10.3% |
| Kanban's wasm | 603 KB (193 KB gzip) | 539 KB (191 KB gzip) | −11% raw, −1% gzip |
| Frontend wasm (`frontend-demo`) | 1.95 MB (605 KB gzip) | 1.72 MB (594 KB gzip) | −12% raw, −2% gzip |

`-Os` and `-O3` came within 1% of `-Oz` gzipped. Compression is the win;
wasm-opt mostly saves the browser decompressed bytes to compile.

**Measured.** Medians of three, `angreal e2e twenty-measure` against `angreal demo up
--with twenty --release`, same machine and method as [[HLIN-T-0084]] (Apple
M3 Pro, headless Chromium from Playwright 1.63, loopback, other agents
building on the machine). The three runs agreed within 20 ms and a few KB
(throttled: 1,147 to 1,374 ms).

| | Before ([[HLIN-T-0084]]) | After |
|---|---|---|
| Cold bytes, first screen (9 frames) | 7.73 MB (modules 5.69, shell 2.02) | **2.27 MB** (modules 1.70, shell 0.56) |
| Every module asset once (20 modules), as sent | 12.53 MB | **2.00 MB** (11.32 MB uncompressed) |
| Shell frontend, as sent | 2.01 MB | **0.54 MB** (1.78 MB uncompressed) |
| Warm bytes, first screen | 0.72 MB | **0.05 MB** |
| Cold, navigation → six drawn (first content), loopback | 588 ms | 817 ms (see below) |
| Cold over 50 Mbit/s, 40 ms → six drawn | 1,749 ms | **1,373 ms** |
| Warm → six drawn | 535 ms | 537 ms |

**Loopback got slower, and why.** The demo's shell is a debug binary (the
twenty flavour signs in with `dev`, which a release shell refuses), and a
debug build of brotli is about ten times slower than an optimised one:
brotli at quality 4 on the frontend's 1.72 MB wasm takes 223 ms in the
demo's shell and 22 ms from the `brotli` CLI. On loopback, where bytes are
free, that is all cost; over a 50 Mbit/s link the saving wins by 380 ms. A
release shell should get the loopback time back. Compressing each asset
once and keeping it (per file and encoding, or `ServeDir`'s precompressed
files for the frontend) would remove the cost altogether; not done here.

Checks: `angreal check all` clean; `angreal test all` 1023 passed;
`angreal ui build`; standard suite (`--with aurora`) 71 passed, 28 skipped,
1 failed (the known `streams.spec.js` "stops pulling", [[HLIN-T-0089]]);
`angreal e2e signin` 9 passed; `angreal e2e twenty` 6 passed;
`twenty-measure` 4 passed.
