---
id: prove-a-module-runs-and-cannot
level: task
title: "Prove a module runs, and cannot escape, in a real browser"
short_code: "HLIN-T-0071"
created_at: 2026-09-25T00:01:10.232071+00:00
updated_at: 2026-09-25T12:02:02.482292+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Prove a module runs, and cannot escape, in a real browser

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 11 of [[HLIN-I-0011]]. NFR-1.1 and NFR-1.3 of [[HLIN-S-0007]].

## Acceptance Criteria

## Acceptance Criteria

- [ ] A small Leptos module on `hlin-sample-platform`, built with the SDK:
      follows the time picker, makes a read and a write as the viewer, and
      announces `changed`
- [ ] The demo builds it (`angreal ui build` or its own task) and `demo up`
      serves it
- [ ] Browser tests that it draws, follows context, and that a write by one
      viewer reaches another's module
- [ ] Containment matrix, settled and written into [[HLIN-S-0007]], asserting
      a hostile test module cannot: read the parent document, read cookies or
      storage, fetch the network, navigate its frame off `/m/`, call `/p/`
      directly, reach another platform's prefix, flood messages past the rate,
      or starve the page with streams
- [ ] Load time measured: a cached module `ready` within 1 s; six modules from
      three platforms interactive within 3 s cold. Recorded in the task and
      failing the test if missed
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on everything else in [[HLIN-I-0011]].
- Mind the `e2e-frontend-must-match` memory: the demo flavour the tests expect
  must be the one `demo up` ran.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
