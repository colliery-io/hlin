---
id: warn-in-a-debug-build-when-a
level: task
title: "Warn in a debug build when a module holds the main thread too long"
short_code: "HLIN-T-0090"
created_at: 2026-09-25T18:44:43.115834+00:00
updated_at: 2026-09-25T18:44:43.115834+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


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

- [ ] In a debug build of a module, the SDK observes long tasks (a
      `PerformanceObserver` for `longtask` where the browser has it, else a
      timer-drift check) and logs a warning to the console naming the
      duration, with a pointer to the rule
- [ ] Nothing is observed or logged in a release build
- [ ] The SDK's crate docs and the README's module section state the rule:
      never block the main thread; chunk long work or use a Web Worker from
      the module's own assets
- [ ] A browser test with a module that busy-loops for 300 ms sees the
      warning in the frame's console
- [ ] `angreal check all`, `angreal test all` pass

## Status Updates

### 2026-09-25

Created from the owner's decision on hung modules. Not started.
