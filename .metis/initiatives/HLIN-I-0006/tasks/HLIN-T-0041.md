---
id: the-browser-suite-fails-in-a-full
level: task
title: "The browser suite fails in a full run and passes file by file"
short_code: "HLIN-T-0041"
created_at: 2026-09-08T02:40:00+00:00
updated_at: 2026-09-08T10:27:10.485754+00:00
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

# The browser suite fails in a full run and passes file by file

## Objective

`angreal e2e test` reports 21 of 22. `surface.spec.js` "removing a panel takes
it off the grid for good" finds zero panels where two should be. The same file
run alone passes all twelve.

Found while working [[HLIN-T-0027]], and confirmed not to be that change: with
the change stashed and the binary rebuilt, the failure is identical.

## Backlog Item Details

### Type
- [x] Tech Debt (test reliability)

### Priority
- [x] P1 - High: the suite is the evidence every other task in this initiative
      relies on, and it currently cannot be trusted in the form CI would run it

### Technical Debt Impact
- **Current Problems**: The suite's verdict depends on what ran before it. That
  was a one-in-three flake when recorded in [[HLIN-I-0004]] (then in
  `surface.spec.js` "resizing", timing out waiting for any panel); it is now
  consistently reproducible in a full run. The symptom is the same both times —
  a surface renders no panels at all — which suggests one cause rather than two.
- **Benefits of Fixing**: A red suite means something, and `retries: 0` becomes
  defensible again.
- **Risk Assessment**: A suite that fails for unrelated reasons gets ignored,
  and then it hides a real regression. That has already happened once this
  session with a Rust test.

### What is known
- Both symptoms are "the surface has no panels", not a wrong assertion about
  panels that exist.
- `surface.spec.js` shares one layout across its twelve tests via `beforeAll`
  and runs `mode: 'serial'`.
- It reproduces with `live.spec.js` + `surface.spec.js` together, and with the
  full set; `live.spec.js` alone passes and measures 6.5Hz.
- The suite runs `workers: 1`, `fullyParallel: false`, `retries: 0`.

## Acceptance Criteria

## Acceptance Criteria

- [ ] The cause is named, not worked around — in particular, whether the layout
      genuinely has no panels at that moment, or the browser failed to render
      panels it was sent
- [ ] `angreal e2e test` passes ten consecutive full runs
- [ ] Whatever the cause, a test that depends on state an earlier test left is
      either made independent or has that dependency stated in the file

## Implementation Notes

### Technical Approach
Start by asking the API what the shared layout holds at the moment of failure,
rather than inferring from the page: that separates "the write was lost" from
"the render was lost", and those have nothing in common.

### Dependencies
None. Worth doing before the remaining tasks in [[HLIN-I-0006]], since each of
them is verified by this suite.

## Status Updates

### 2026-09-08 — investigated, cause partly named, not fixed

**Not completed.** The acceptance criteria are not met and this task stays open.
What follows is what the evidence shows, so the next attempt does not start
from nothing.

**It is not deterministic.** I recorded it as deterministic after two identical
failures; that was wrong. Across runs it lands on three different tests —
"resizing from the corner", "removing a panel", and `live.spec.js` — and a run
immediately after a shell restart passed all 22 with no warning or error in the
shell log at all.

**It is not caused by [[HLIN-T-0027]].** Stashed the change, rebuilt, ran the
full suite: identical failure.

**The symptoms are not one symptom.** Three distinct ones, which is why a single
cause has not been found:

| Failure | What it saw |
|---|---|
| `resizing from the corner` | `.corner` had no bounding box — "the drag handle is not on screen" |
| `removing a panel` (first form) | `toHaveCount(2)` saw 0 for the full 30s |
| `removing a panel` (second form) | `toHaveCount(1)` saw 2 — the drop click did not take |

The first is the test outrunning the stream. The second is panels never
arriving at all. The third is a click that did not register. Only the first is
obviously a test problem.

**Configuration matters, and not the way I guessed.** I hypothesised that
running against `--with live` (8Hz) destabilised the click tests, since panels
redraw about seven times a second under the cursor. Tested it: five runs
against `--with gallery` (30s refresh) were *worse* — 1, 1, 2, 2, 1 failures —
not better.

That result is itself the useful finding: **`live.spec.js` asserts a redraw rate
above 3Hz, which the default 30-second configuration cannot produce.** So the
suite's files disagree about which demo they need. `live.spec.js` requires a
fast shell; nothing else does; and `angreal e2e test` runs whatever happens to
be up without saying which it expects. At least one failure in every gallery run
is `live.spec.js` failing honestly.

**One fix made**, because it is correct regardless: `dragBy` now waits for the
handle to be visible before measuring it. `boundingBox()` does not retry the way
an assertion does, so a panel that has not arrived yet failed as "the drag
handle is not on screen" — which reads like a layout bug and is really a race.

### Where to go next

1. `/api/config` should report the refresh interval. Then `live.spec.js` can
   skip honestly on a shell that cannot deliver what it asserts, instead of
   failing. This overlaps [[HLIN-T-0033]], which is the same shape of problem —
   a timing value the browser needs and is not told.
2. `angreal e2e test` should state and check which configuration it expects,
   rather than running against whatever is up.
3. The remaining two symptoms — panels never arriving, and a click that does not
   register — need the diagnostic the ticket already describes: ask the API what
   the layout holds at the moment of failure, to separate "the write was lost"
   from "the render was lost".

### 2026-09-08 (later) — three causes fixed, one residual, still not closed

Working [[HLIN-T-0033]] closed two of the three symptoms and a third turned up
on the way. Failures are down from one or two in *every* run to roughly one run
in five. The ten-consecutive criterion is still not met, so this stays open.

**Fixed, each with its own cause:**

1. `live.spec.js` asserted a redraw rate the default configuration cannot
   produce. `/api/config` now reports `refresh_ms` and the test skips honestly
   rather than failing. This was one failure in every gallery run.
2. `angreal demo up` reported ready when `/api/health` answered, which is before
   the registry has polled any platform — so `/api/panels` was empty and a
   layout naming a panel rendered nothing. That is the "panels never arrived"
   symptom exactly. `up` now waits until a platform is actually offering panels.
   Same family as the readiness defect fixed earlier in this session.
3. `dragBy` measured a handle without waiting for it.

**Evidence after:** seven consecutive clean runs in one batch (21 passed, 1
honestly skipped), then a batch of five where the first failed and four passed.

**What is left**, and it is now the only thing: roughly one run in five fails,
and not on a restart boundary — the last occurrence was the first of five
consecutive runs against an already-warm shell. The artifact is cleaned by the
next passing run, so catching it needs `--reporter=html` retained, or a run loop
that stops on first failure and preserves `results/`.

## Status Updates

### 2026-09-08 — Found: the write was cancelled by the test's own reload

Two distinct causes, both the same shape — a suite that depended on something
it never said out loud.

**The removal failure is a real product defect, not a flaky click.** The trace
settles what a screenshot could not: the click landed, the panel left the grid,
the first `toHaveCount(1)` *passed*, and the count went back to 2 only after
`page.reload()`. The network log names it — `PUT /api/layouts/{id}` with status
`-1`, aborted. The browser cancelled the write on navigation.

So the diagnostic the ticket asked for — "was the write lost or was the render
lost?" — answers *the write*, and the reason is that composition is optimistic:
the draft changes when the button is pressed and the write follows. Between
those two moments the surface is a promise, and the suite navigated through the
gap. Under a busy shell the gap is wider, which is exactly why this failed in a
full run and passed file by file.

**A person hits this too.** Remove a panel, reload immediately, and it is back,
with nothing having said the change was still in flight. The fix is therefore
in the product first: the bar now says `saving…` while a write is outstanding,
next to the stream's own `live`/`reconnecting`. Both answer the same question
from the two directions — is what I am looking at true?

The suite then has something honest to wait on: `settled(page)` in `helpers.js`,
used by every test whose gesture is supposed to outlive the page — which is
more of them than the ones that reload, because ending a test closes the page
and cancels the write just the same. Three tests (add, drag, resize) were
ending dirty and failing *the next* test rather than themselves.

**The components failure was a hidden configuration dependence.** `?pack=aurora`
is honoured only by a frontend built with more than one pack, so the suite
silently required `demo up --with gallery` and, under the default config, failed
as though the component seam were broken. The mounted pack now publishes what it
answers to as `data-offers` on the stage, and both halves of the seam test skip
with a message naming the flag — the shape `live.spec.js` already used.

Publishing `data-offers` is worth having on its own: whether a pack drew a
component or declined it is the one thing about a panel that looking at the
panel cannot settle, because a pack that declines draws the declared kind, and
so does a pack that never heard of components.

Bar for closing this is ten consecutive clean runs.
