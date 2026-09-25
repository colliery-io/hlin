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


exit_criteria_met: false
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

- [ ] The shell compresses (gzip, and brotli where the browser accepts it)
      its own frontend and whatever it passes through under `/m/` that the
      platform did not already compress, honouring `Accept-Encoding`, with a
      sensible minimum size and correct `Vary`
- [ ] Compression does not break streaming responses under `/p/`, and is not
      applied to them
- [ ] Module and frontend release builds run `wasm-opt` (Trunk's
      `data-wasm-opt`), and the size change is recorded
- [ ] The twenty measurement re-run: cold bytes for the first screen and all
      twenty, before and after, recorded in [[HLIN-I-0012]]'s results
- [ ] `angreal check all`, `angreal test all`, and every browser suite pass

## Implementation Notes

- `crates/hlin/src/modules/assets.rs`, `server.rs` (frontend serving),
  tower-http's compression layer if it fits the bounded-body reading;
  `index.html` of each module and frontend for `data-wasm-opt`.

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.
