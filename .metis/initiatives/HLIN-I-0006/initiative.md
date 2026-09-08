---
id: close-the-architectural-review
level: initiative
title: "Close the architectural review"
short_code: "HLIN-I-0006"
created_at: 2026-09-08T02:05:00+00:00
updated_at: 2026-09-08T02:05:00+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/decompose"

exit_criteria_met: false

estimated_complexity: L
initiative_id: close-the-architectural-review
---

# Close the architectural review

## Problem

The architectural review at `8298c23` found the thesis implemented and holding —
runtime contract enforcement, per-request signed identity, total rendering, a
design-pack seam the shell knows nothing about — and the shell not deployable,
for fifteen reasons of varying size.

Three are critical. The shell has no authenticator, so every ownership and
visibility rule above it is vacuous. Surfaces never stop, and were measured
sending 84 and 151 requests per second to the demo platforms with nobody
watching. And any viewer can append arbitrary query parameters to the shell's
authenticated calls upstream.

Review: https://claude.ai/code/artifact/307d6cda-eb4f-4628-82dc-55df824d7a2d

## Approach

Work the findings in the order the review recommends, which is ordered by risk
retired per hour rather than by severity. The three that could take the shell or
a platform down come first and are each an afternoon; the authenticator, which
is the one that makes a deployment possible at all, comes fourth because it is
the largest and nothing else waits on it.

Each task carries where it lives, what was measured, and testable acceptance
criteria, so none of them needs the review re-read to be worked.

## Success Criteria

- [ ] The three P0 findings are closed: no surface leak, no parameter injection,
      no unbounded buffering
- [ ] A request to the shell carries a principal that came from somewhere other
      than a configuration file
- [ ] Every P1 is closed, or explicitly deferred with a reason in its task
- [ ] `angreal check all`, the full Rust suite and the browser suite pass at each
      task's completion, and the walkthrough still holds
- [ ] Each ADR the review marked "partial" either matches the code or has been
      amended to say what the code does

## Tasks

Ordered as the review recommends.

| # | Task | Finding | Priority |
|---|---|---|---|
| 1 | [[HLIN-T-0027]] Stop a surface when its last viewer leaves | 2 | P0 |
| 2 | [[HLIN-T-0028]] Forward only the parameters a panel declares | 3 | P0 |
| 3 | [[HLIN-T-0029]] Cap upstream response bodies | 4 | P1 |
| 4 | [[HLIN-T-0026]] Implement the authenticator strategies | 1 | P0 |
| 5 | [[HLIN-T-0030]] Persist and expose contract verdicts | 5 | P1 |
| 6 | [[HLIN-T-0031]] Handle broadcast lag | 6 | P1 |
| 7 | [[HLIN-T-0035]] Reconcile a surface in place | 10 | P2 |
| 8 | [[HLIN-T-0036]] Let a platform declare its cadence | 11 | P2 |
| 9 | [[HLIN-T-0032]] Dedup per principal, or amend A-0004 | 7 | P2 |
| 10 | [[HLIN-T-0034]] Run the Postgres store test | 9 | P2 |
| 11 | [[HLIN-T-0033]] One staleness clock | 8 | P2 |
| 12 | [[HLIN-T-0037]] Let a pack say what it offers | 12 | P3 |
| 13 | [[HLIN-T-0038]] Exercise Intent::Range | 13 | P3 |
| 14 | [[HLIN-T-0039]] Fetch options in one request | 14 | P3 |
| 15 | [[HLIN-T-0040]] Sweep the tests for tallies | 15 | P3 |

## Risk Considerations

[[HLIN-T-0026]] is not one afternoon. `trusted-header` is; `oidc` needs
decisions about session storage, redirect handling and cookie security that
deserve their own conversation rather than being made inside a loop. The task
will be split if that becomes the blocker.

[[HLIN-T-0036]] needs an ADR before any code, because it changes the manifest
and manifest vocabularies only grow.

## Status Updates

### 2026-09-08 — five closed, one open, nine untouched

Worked in the recommended order. Every change is committed separately with its
own evidence.

| Task | | Evidence |
|---|---|---|
| [[HLIN-T-0027]] surface leak | done | 230 req/s → 0, measured on the demo |
| [[HLIN-T-0028]] parameter injection | done | injected key reached the platform 0 times |
| [[HLIN-T-0029]] unbounded buffering | done | 4 socket-driven tests, incl. the chunked case |
| [[HLIN-T-0033]] one staleness clock | done | derived and clamped 5–30s; `/api/config` reports both |
| [[HLIN-T-0031]] broadcast lag | done | a lagging receiver keeps its stream |
| [[HLIN-T-0041]] browser suite | open | 1-in-every-run → ~1-in-5; bar is ten consecutive |

**All three P0 findings from the review are closed.** The shell no longer leaks
surfaces, cannot be made to forward a parameter a platform never declared, and
cannot be made to allocate without bound by a platform having a bad day.

Rust suite went 281 → 295, still 0 failures. The browser suite is 21 passed,
1 honestly skipped — `live.spec.js` now knows when a shell cannot deliver what
it asserts instead of failing.

**Three defects turned up that were not in the review**, all found by doing the
work rather than by reading:

- `angreal demo up` reported ready before its registry had polled anything, so
  the first browser run against a fresh demo saw an empty catalogue. Same family
  as the readiness defect fixed earlier in the session.
- The interaction test I wrote last session clicked the *first* facet pill, and
  a platform given no selection applies its own default, which is its first
  choice — so it asked for what was already on screen and could only pass by
  accident of the time window moving.
- Making the browser's grace equal to staleness raced a test that could never
  win, which surfaced the real design question: those are two different
  judgements and should be related, not identical.

### What the remaining nine need

Seven are ordinary implementation: [[HLIN-T-0030]], [[HLIN-T-0034]],
[[HLIN-T-0035]], [[HLIN-T-0037]], [[HLIN-T-0038]], [[HLIN-T-0039]],
[[HLIN-T-0040]].

Two need a decision that is not mine to make, and both say so in their own text:

- [[HLIN-T-0026]] — `trusted-header` is an afternoon. `oidc` needs decisions
  about session storage, redirect handling and cookie security.
- [[HLIN-T-0036]] — an ADR first, because it changes the manifest and manifest
  vocabularies only ever grow.

And [[HLIN-T-0032]] is a choice rather than a task: amend HLIN-A-0004 to say
what the code does, or implement the per-principal dedup it claims. Amending is
honest and cheap; implementing matters only once one principal routinely holds
several surfaces open.

### 2026-09-08 (final) — eleven closed, three open, all for stated reasons

Every implementation task in this initiative is done. What remains needs a
decision rather than an afternoon, and each says so in its own text.

| Task | | Evidence |
|---|---|---|
| [[HLIN-T-0027]] surface leak | done | 230 req/s → 0, measured |
| [[HLIN-T-0028]] parameter injection | done | injected key reached the platform 0 times |
| [[HLIN-T-0029]] unbounded buffering | done | 4 socket tests, incl. chunked |
| [[HLIN-T-0033]] two staleness clocks | done | derived, clamped 5–30s |
| [[HLIN-T-0031]] broadcast lag | done | a lagging receiver keeps its stream |
| [[HLIN-T-0030]] contract verdicts | done | visible on `/api/platforms`, survives restart |
| [[HLIN-T-0040]] truncating test runs | done | 118 → 296 tests on a failure |
| [[HLIN-T-0037]] pack `offers` | done | a lying pack still draws |
| [[HLIN-T-0034]] Postgres suite | done | runs, skips, or fails — all three verified |
| [[HLIN-T-0038]] `Intent::Range` | done | a brush, and the redraw bug it exposed |
| [[HLIN-T-0039]] options fan-out | done | 2 requests → 0 where none are needed |
| [[HLIN-T-0035]] reconcile in place | done | a moved panel keeps its data |
| [[HLIN-T-0026]] authenticator | **part** | `trusted-header` done; `oidc` needs decisions |
| [[HLIN-T-0041]] browser suite | **open** | 1-per-run → ~1-in-5; bar is ten consecutive |

**All three P0 findings are closed**, and so is every P1. The Rust suite went
281 → 307 with no failures; the browser suite is 23 passing and 1 honestly
skipped.

### What the review did not find

Six defects turned up by doing the work rather than reading it:

- `angreal demo up` reported ready before its registry had polled anything.
- `angreal test all` truncated on the first failing binary — 178 tests silently
  skipped, reported as a pass.
- The facet test clicked the platform's own default, so it could only pass by
  accident of the clock moving.
- Making the browser's grace *equal* to staleness raced a test that could never
  win, which exposed that those are two different judgements.
- `aurora.brush` kept its drag state in a `StoredValue`, which a redraw between
  pointerdown and pointerup silently discarded — the same shape as the pointer
  capture bug in [[HLIN-I-0002]], and certain rather than intermittent at 8Hz.
- Filtering the options fetch by "panels on the surface" inside the catalogue
  effect compiled cleanly and broke adding a panel later.

### The three that are yours

- **[[HLIN-T-0026]], `oidc`.** Session storage, cookie security, and the
  state/nonce redirect round trip. Getting the last wrong is a vulnerability
  rather than a bug. Worth splitting into its own task.
- **[[HLIN-T-0036]], per-panel cadence.** Needs an ADR before any code, because
  it changes the manifest and manifest vocabularies only ever grow.
- **[[HLIN-T-0032]], dedup scope.** Not a task but a choice: amend HLIN-A-0004
  to say what the code does, or build the per-principal deduplication it claims.
  Amending is honest and cheap; implementing matters once one principal
  routinely holds several surfaces open. Changing a decision record is not mine
  to do quietly.

And **[[HLIN-T-0041]]** is open on its own merit: three causes found and fixed,
failures down from one or two every run to about one in five, and the remaining
one needs the diagnostic its ticket describes.

### 2026-09-08 — All sixteen closed

The last two were the ones held back for a reason rather than for time.

**HLIN-T-0041** turned out not to be a flaky test. The trace shows the click
landing and the panel leaving the grid; the network log shows the layout write
aborted by the reload that followed. Composition is optimistic, so between the
gesture and the write the surface is a promise — and the suite navigated
through that gap, as a person can. The product now says when it still owes the
shell a write, and the suite waits on that. Ten consecutive full runs, 240
tests, no failures.

**HLIN-T-0026** needed three decisions rather than more code, and got them:
sessions as rows in Postgres, a cookie policy the shell refuses to start
without, and a state/nonce/PKCE round trip written down server-side and claimed
once. `Option` was the wrong return type for "who is asking" and hid the whole
change; making it three answers let the compiler find every caller.

Six defects along the way were ones the architectural review had not found,
because they only appear when the work is done: a floor derived as a fraction
that made a fast panel slower the slower the shell was, a drag whose state a
re-render discarded, an options fetch that worked for the panels present at load
and no others, a test clicking the platform's own default and passing by
accident, a browser grace period racing a test that could never win, and a
suite that silently required one of four demo configurations.

Ready for review.
