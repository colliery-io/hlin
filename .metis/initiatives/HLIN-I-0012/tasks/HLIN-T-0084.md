---
id: measure-twenty-platforms-on-one
level: task
title: "Measure twenty platforms on one surface, and prove it degrades one at a time"
short_code: "HLIN-T-0084"
created_at: 2026-09-25T02:43:04.052973+00:00
updated_at: 2026-09-25T02:43:04.052973+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# Measure twenty platforms on one surface, and prove it degrades one at a time

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 5 of [[HLIN-I-0012]]: the numbers the new bet asked to be watched,
and proof that twenty independent platforms fail independently.

## Acceptance Criteria

- [ ] Browser tests on "Twenty": every widget reaches `ready`; scrolling top
      to bottom never has more than 12 frames mounted while none in view is
      ever unmounted; a scrolled-away widget with state gets it back on
      return (`suspend`/`state`)
- [ ] Kill one widget's process: its panel alone goes `stale` then
      `unavailable` (or falls back), and the other nineteen are unaffected;
      restart it and it recovers
- [ ] A shared widget changed in one browser updates in another, and the
      time from write to the other browser is recorded
- [ ] Numbers recorded in this task and in [[HLIN-I-0012]]: cold and warm
      time from navigation to every in-view widget `ready`; total bytes
      transferred cold and warm; JS heap and process memory with the surface
      open; mounted-frame count over a scroll. Measured three times, median
      reported, machine described
- [ ] Compared against [[HLIN-S-0007]] NFR-1.1 (six modules interactive
      within 3 s cold), with a plain statement of whether twenty fits the
      bet, and what would have to change if not
- [ ] Runnable by an angreal task

## Implementation Notes

- Depends on [[HLIN-T-0081]], [[HLIN-T-0082]], [[HLIN-T-0083]].
- Playwright can read `performance` entries and, in Chromium, CDP metrics
  for memory.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.

### 2026-09-25 — measure release builds

The owner: measure with release builds, to be accurate. `angreal demo up
--release` now builds the browser's WebAssembly optimised (the frontend and
every module); the build profile is part of each module's fingerprint, so
switching profiles rebuilds. Shell and platform binaries stay debug, because
the twenty flavour signs in with `dev`, which a release shell refuses; they
serve bytes and are not what the browser pays for.

First numbers, three widgets, on the T-0080 branch:

| | Debug | Release |
|---|---|---|
| Module wasm, each | 2.71–3.08 MB (0.62–0.68 MB gzip) | 0.55–0.61 MB (0.18–0.19 MB gzip) |
| Shell frontend wasm | 8.18 MB | 1.89 MB |
| Three widgets `ready`, fresh browser context | about 1.2 s | 1.03 s |
| Cold module build, three widgets | 16.8 s | 23.3 s |

Twenty release modules should be about 11–12 MB uncompressed, 3.7 MB gzipped,
if the other seventeen are similar. Whether the shell and the platforms
compress what they serve is worth checking in this task: the asset proxy
passes bytes through as the platform sent them.
