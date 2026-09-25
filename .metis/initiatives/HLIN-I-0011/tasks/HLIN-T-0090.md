---
id: warn-in-a-debug-build-when-a
level: task
title: "Warn in a debug build when a module holds the main thread too long"
short_code: "HLIN-T-0090"
created_at: 2026-09-25T18:44:43.115834+00:00
updated_at: 2026-09-25T21:45:26.474410+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Warn in a debug build when a module holds the main thread too long

## Parent Initiative

[[HLIN-I-0011]]

## Objective

The owner accepted, on 2026-09-25, that a module which spins can freeze the
page outside full Chromium, and put the duty on modules not to block their
main thread ([[HLIN-S-0007]] *Open Questions*, [[HLIN-A-0014]] *Negative*).
A duty nobody is told about is not a duty. The SDK should tell a module's
author, in development, when their module does it.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] In a debug build of a module, the SDK observes long tasks (a
      `PerformanceObserver` for `longtask` where the browser has it, else a
      timer-drift check) and logs a warning to the console naming the
      duration, with a pointer to the rule
- [x] Nothing is observed or logged in a release build
- [x] The SDK's crate docs and the README's module section state the rule:
      never block the main thread; chunk long work or use a Web Worker from
      the module's own assets
- [x] A browser test with a module that busy-loops for 300 ms sees the
      warning in the frame's console
- [x] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created from the owner's decision on hung modules. Not started.

### 2026-09-25 — done

- **SDK.** `hlin-module/src/watch.rs`, declared under
  `cfg(debug_assertions)` and started by `connect`, so a release build has
  none of it. Where `PerformanceObserver.supportedEntryTypes` has
  `longtask` (Chromium), the frame's own long tasks are measured (entry name
  `self`: a frame sharing the page's process hears of the page's and its
  siblings' too). Elsewhere (Firefox, WebKit) a 50 ms interval that fires
  late by the threshold or more is the sign, skipped while the document is
  hidden and reset on `visibilitychange`, since browsers slow hidden timers
  on purpose; that warning says it cannot tell this module from the page or
  another module sharing the thread.
- **Threshold 200 ms**, not the web's 50 ms long task: a debug build is
  unoptimised, often several times slower than its release, and a warning
  that fires on ordinary development work is one people learn to ignore.
  200 ms is twice the 100 ms within which a click should be answered, so
  whatever holds the thread that long is felt in a release build too.
  **At most one warning every 10 s**; the next counts what was held back,
  and the longest.
- **Docs.** Crate docs *Never block the main thread*; `connect`'s docs;
  README *Modules*.
- **Browser test.** The annotations module offers `window.annotations.spin(ms)`
  (a busy loop in a task of its own). `annotations.spec.js` spins it 300 ms
  and expects one `hlin-module:` console warning naming 250–999 ms and the
  rule, or none against `demo up --release` (read from `build.json`). As
  run, debug: Chromium headless shell 299 ms and full 300 ms (measured),
  Firefox about 276 ms and WebKit about 292 ms (late timer).
- **Release.** `the_main_thread_watch_is_compiled_into_a_debug_build_only`
  looks for the rule's words in its own test binary: there in debug, absent
  under `cargo test -p hlin-module --lib --release` (run; passes). Not run:
  the browser test against a `--release` demo.
- `angreal check all`, `angreal test all` (1039 passed, 3 ignored),
  `angreal ui build`, the standard suite `--with aurora` (77 passed,
  28 skipped; the new test is the 77th), and `annotations.spec.js` in all
  four browsers (16) pass.
