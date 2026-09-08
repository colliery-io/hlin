---
id: sweep-the-tests-for-hardcoded
level: task
title: "Sweep the tests for hardcoded tallies"
short_code: "HLIN-T-0040"
created_at: 2026-09-08T01:56:23.965609+00:00
updated_at: 2026-09-08T04:15:56.951003+00:00
parent: HLIN-I-0006
blocked_by: []
archived: false

tags:
  - "#task"
  - "#tech-debt"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0006
---

# Sweep the tests for hardcoded tallies

## Objective

Three tests asserted how many panels the sample platform happened to have on the
day they were written — `surface.spec.js` (12 offers, 6 search hits) and
`hlin-sample-platform/tests/integration.rs` (6 panels). Each broke when a panel
was added, and one went unnoticed for two commits.

Finding 15 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (test hygiene)

### Priority
- [x] P3 - Low

### Technical Debt Impact
- **Current Problems**: A tally asserts nothing true about the behaviour under
  test and fails whenever the demo grows. The `integration.rs` one went unnoticed
  because a failing test binary stops the workspace run, and the command used to
  total results mis-parsed cargo's `FAILED.` line — so two runs were reported as
  "209 passing, 0 failed" when they were truncated and one was failing.
- **Benefits of Fixing**: Adding a panel stops breaking unrelated tests, and a
  failing run is impossible to mistake for a passing one.
- **Risk Assessment**: Low, but it is the failure mode that hid a real defect.

## Acceptance Criteria

- [x] Every remaining `toHaveCount(N)` and `assert_eq!(…len(), N)` over
      platform-declared things is reviewed; those asserting a tally rather than a
      property are rewritten to count from the document or the API
- [x] `angreal test all` fails loudly, naming the failing crate, rather than
      truncating the run and reporting a partial total

## Implementation Notes

### Technical Approach
The three named are already fixed (commits `171a455` and `8298c23`); the sweep
for others is what remains.

### Dependencies
None.

## Status Updates

### 2026-09-08 — swept; the truncation was the real defect

**The sweep found nothing further to fix**, and that is a finding rather than a
shrug. The distinguishing question is whether a number describes something the
*test* controls or something a *platform declares*:

- 22 `toHaveCount(N)` in the browser suite: every one counts panels the test
  itself wrote, facets it selected, or the four time presets, which are a `const`
  in `app.rs`. `surface.spec.js:62` asserting two platforms is the closest call
  and it stays — the two lines after it name Orebank and Stampmill, so the claim
  is "two platforms that know nothing about each other", and a third appearing
  should make somebody look.
- 20 `assert_eq!(…len(), N)` in Rust: all against fixtures the test builds in the
  same function.

The harmful kind — counting what a platform declares, which grows every time the
demo gains a panel — were the three already fixed.

**The second criterion was the one that mattered**, and it is fixed:
`angreal test all` now passes `--no-fail-fast`.

Measured, by adding one deliberately failing test to an early crate:

```
cargo test --workspace                  118 tests ran, then stopped
cargo test --workspace --no-fail-fast   296 tests ran
```

**178 tests silently skipped.** That is the mechanism, exactly: a partial run
reports a partial total, which reads just like a full one. It is why a failing
test survived two commits while the runs that truncated on it were reported as
passing — the count was not merely wrong, it was wrong in the direction that
looks fine.

The probe was removed with `git checkout` and the tree confirmed clean; 296
passing, 0 failed through `angreal test all`. `angreal check all` clean.
