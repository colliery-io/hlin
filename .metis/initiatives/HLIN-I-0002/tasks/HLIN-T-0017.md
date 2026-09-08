---
id: browser-tests-see-the-surface-and
level: task
title: "Browser tests: see the surface, and prove a person can compose one"
short_code: "HLIN-T-0017"
created_at: 2026-09-07T18:38:59.233494+00:00
updated_at: 2026-09-07T20:24:58.405509+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Browser tests: see the surface, and prove a person can compose one

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Close the one gap that has run through [[HLIN-T-0014]], [[HLIN-T-0015]] and
[[HLIN-T-0016]]: no browser has ever rendered this frontend. Every claim about
the surface so far rests on HTTP status codes and unit tests over the arithmetic
underneath the gestures. Nothing has established that a person opening the page
sees panels, that dragging one moves it, or that the picker does anything at
all.

Playwright drives a real browser against the running demo, asserts what should
be on screen, and captures screenshots at each step. The screenshots are the
part a person can check in a second; the assertions are the part that catches a
regression six months from now.

## Acceptance Criteria

## Acceptance Criteria

- [x] A Playwright project under `e2e/`, with its own `package.json` and config, pinned, and not entangled with the Rust workspace
- [x] An angreal `e2e` group: `install` fetches Playwright and its browser, `test` runs the suite against a running demo, `shots` runs it and reports where the screenshots landed
- [x] The suite creates its own layout through the API and works in that, so a run neither depends on nor clobbers whatever surface is already there
- [x] Composition, asserted in a browser: the picker lists both platforms; adding a panel puts it on the grid; the panel reaches a drawn state with content; dragging by the handle moves it to a different grid position; resizing from the corner changes its span; a reload shows the same arrangement
- [x] The shell's own controls, asserted in a browser: the time picker marks a preset active and the applied indicator settles; switching a panel's kind changes how it is drawn; removing a panel takes it off the grid
- [x] A screenshot per step, written to a known directory, named so the sequence reads in order
- [x] `angreal e2e test` exits non-zero when an assertion fails, and the failure names the step
- [x] The screenshots are actually looked at, and what they show is written down — including anything that turns out to be wrong

## Implementation Notes

### Technical Approach
Playwright rather than the Chrome extension, because this has to run without a
person present and has to keep working after this session. JavaScript rather
than TypeScript, so there is no build step between the test and the browser.

The frontend is WebAssembly and the panels arrive over a stream, so nothing is
ready on load. Every assertion waits on a condition rather than a timeout, and
the waits are generous, because a panel's first frame is behind a fetch to a
platform.

### Dependencies
A running demo: [[HLIN-T-0016]]'s `angreal demo up`. The suite asserts against
that rather than starting its own, so what it exercises is the thing a person
would open.

### Risk Considerations
The obvious trap is a suite that passes because it asserts nothing: waiting for
an element that is always present, or screenshotting before the page has drawn.
Each assertion should name a state that could plausibly be wrong, and the
screenshots should be looked at rather than filed.

## Status Updates

### 2026-09-07 — the first run, and what looking at it found

Playwright under `e2e/`, pinned to 1.63, driving Chromium against a demo
started with `angreal demo up`. Fifteen tests across two files, a screenshot per
step, and an angreal `e2e` group with `install`, `test`, `shots` and `clean`.

**The frontend works.** This is the first time it has been rendered anywhere.
The picker lists both platforms with six panels each and the kind beside every
name, the bar carries the presets and the two date boxes, the stream reports
itself live, and edit mode swaps the button to Done. Nothing about the layout or
the styling is broken.

**One defect, found by looking at a screenshot rather than by a test.** The
applied indicator sat on "applying…" in orange and never cleared. The cause is
an ordering problem the unit tests could not see: the browser posts its
parameters as soon as the layout is known, the shell acknowledges by
broadcasting a surface frame, and the browser has not subscribed yet — so the
acknowledgement goes to nobody. `current_frames` then sent panel frames on
subscribe but no surface frame, so the browser never learned its change had
landed and reported the surface as pending for as long as the tab stayed open.

Fixed by making the surface frame part of "the current state of everything",
which is what the specification always meant: a browser that subscribes should
be correct without replay, and which question the surface is answering is part
of being correct. A panel frame cannot carry that, because it says what one
panel is doing, not what generation is in force. One test.

**Two failures that were my tests being wrong, not the product.**

- The search test expected four matches for "throughput" and got six. Three per
  platform, two platforms. Corrected, and given the assertion it was missing:
  that a search matching nothing shows nothing.
- The stream-loss test used `context.setOffline(true)` and expected the
  connection to drop. Chromium leaves an established `EventSource` alone, so the
  page stayed live and the assertion timed out. Rewritten to refuse the stream
  request before the page loads, so the connection fails on open. The
  ready-to-stale step is covered by a host test in `state.rs`; what the browser
  test now covers is the visible end of it, that the panels end up unavailable
  and name Hlin rather than a platform.

**A trap the task's own risk note predicted, walked into anyway.** The first
degradation test asserted that no panel was in the `waiting` state and passed in
388ms — while the screenshot showed three panels of grey loading skeletons.
`waiting` and `loading` are different states, so the assertion was true and
meaningless. It now waits for `ready` and checks each panel drew a number, a
chart or a table.

### 2026-09-07, second run — the defect that mattered

The rewritten tests failed on something real, and it is the worst bug found in
this initiative.

**Every panel with a time range broke the moment anybody used the time picker.**
A timeseries panel showed "this platform sent something unreadable" while the
scalar and table panels beside it were fine.

The cause is one missing call. `Surface::query_for` built the query string by
concatenating raw values:

```
from=2026-09-07T18:00:00+00:00&to=...&step=6
```

An RFC 3339 offset carries a `+`. A `+` in a query string means a space. The
platform's date parser received `2026-09-07T18:00:00 00:00`, refused it, and
answered 400 — and axum rejects at the extractor, before the handler, so the
platform logged nothing at all. The shell mapped 400 to `malformed` and told
the viewer the platform had sent something unusable. It had not. The shell had
asked the question wrong and then blamed the answer.

Confirmed three ways against a live platform: the raw form 400s, the
percent-encoded form and the `Z` form both 200.

The fix percent-encodes names and values, and formats instants with `Z`. The
encoder is eleven lines rather than a dependency. Sorting still happens before
escaping, so two instances that chose the same thing still share one fetch —
there is a test for that, because an encoding change is exactly the sort of
thing that quietly defeats deduplication. Two more tests: that no `+` survives
into a query, and that a viewer typing `north & south=all` into a selection
produces one parameter rather than three.

**Why nothing caught this.** The aggregator's own tests asserted the query
contained the right *parts*, never that a platform could parse the result. The
walkthrough changed the time range and asserted the generation advanced and a
panel stayed ready — but the panel it watched was a scalar whose handler has no
query extractor, so it was immune. The sample platform is the reference a real
platform is copied from, and its timeseries handlers are the realistic ones. It
took a browser drawing the panel to make the failure visible.

**A second, smaller defect found while chasing the first.** One of the two paths
that return `malformed` logged nothing, so an operator saw the state and had no
way to find out why. That path now says which endpoint no accepted panel
declares. Finding this cost half an hour of looking at a silent log, which is
its own argument.

### 2026-09-07, third run — two more, both only findable this way

The encoding fix worked: three panels from two platforms drawn in 1.2 seconds,
the timeseries showing three coloured lines, the applied indicator settled.
Adding panels and dragging one both passed. Two tests still failed, and both
were the product rather than the test.

**Resizing did nothing, and left the panel stuck to the pointer.** Dragging a
panel's corner down to make it taller takes the pointer below the grid within
about a hundred pixels, and the `pointermove` and `pointerup` listeners were on
the grid. Past its edge the shell stopped hearing about the gesture: the panel
never resized, and the pointer-up that should have ended the gesture landed on
the document, so the panel kept its dragging state indefinitely.

Pointer capture was supposed to prevent exactly this and does not survive here.
The element captured is inside the panel being redrawn on every move, and a
captured element that gets replaced releases the pointer. Instrumenting the
gesture showed it plainly: the class went to `dragging` on pointer-down and
`data-w` never moved through eight pointer moves, then stayed `dragging` after
pointer-up.

The listeners are now on the window, where nothing has to survive anything, and
the capture call is gone rather than left in as a thing that looks like it is
helping. `pointercancel` is handled too, so a gesture the browser takes away
does not leave a panel stuck.

Dragging worked before this fix only because moving a panel sideways keeps the
pointer inside the grid. The bug was always there; the drag just did not reach
it.

**A panel on a dead stream showed a loading skeleton forever.** With the stream
refused, panels the browser had never received a frame for stayed as grey
skeletons indefinitely. `SurfaceState` only knows panels the stream has
mentioned, so `on_tick` had nothing to act on, and a panel that has never been
mentioned is exactly the case where the stream never opened.

This contradicted a principle: rendering is total over its inputs, and every
panel is in exactly one state at all times. A skeleton that never resolves tells
a person the panel is slow when the shell is gone. `SurfaceState::given_up` now
answers the question the surface has to ask about panels it holds nothing for,
and such a panel draws as unavailable, naming Hlin. Tested on the host,
including that a stream which came back is not given up on however long it was
away.

### 2026-09-07, fourth run — the stream was being thrashed

The degradation suite passed in full. Composition failed on a new symptom:
two panels added, both stuck as skeletons, the stream reporting itself live,
and the applied indicator stuck on "applying…".

The stream effect read the draft to find the surface id, which **tracked** it.
The draft changes on every pointer move of a drag and on every edit, so the
effect re-ran constantly, closing and reopening the `EventSource` each time.
The browser ended up holding a connection it had just closed, receiving nothing,
while the shell fetched away happily on the other side. Earlier runs passed
through this by timing.

The effect now reads the surface id untracked and depends on the revision alone,
which moves once per write — exactly when the surface's panels can have changed.

**A wording contradiction, seen in a screenshot.** With the stream dead the
panels read "this platform is not responding" as the headline and "Hlin is not
responding" as the detail beneath. Both came from a `Cause::Unreachable`, and
the pack's copy named a party it cannot know: a pack is handed a cause and
nothing else, and the unreachable party is sometimes a platform and sometimes
the shell. Naming the wrong one sends a person to look at a system that is
answering perfectly well — the exact failure this test exists to catch, half
made by the fix for it. The pack now says "no answer", and whoever knows names
the party in the detail.

### 2026-09-07, fifth run — a race in the shell, and the worst of the lot

Composition still failed with panels stuck as skeletons, so this time the
gesture was instrumented rather than reasoned about. A probe opened its own
`EventSource` from inside the page and printed what actually arrived:

```
surface {"generation":1,"acknowledged":true}
panel   {"generation":1,"state":"ready", ...}
```

The panel was **ready, and at generation 1**, while the browser had moved to
generation 2 when it applied its time range. A frame from a generation the
browser has passed is dropped, correctly and by design. So the shell was
sending a healthy panel and the browser was throwing it away, both behaving
exactly as specified.

The cause is a check-then-act race in `Surfaces::for_surface`: look in the map,
release the lock, build a surface, insert it. The browser posts its parameters
and opens its stream **from the same tick**, so those two requests arrive
together every single time a layout is written. Both missed the map, both built
a surface, and the second insert won. The one the stream was subscribed to was
orphaned the moment it was replaced — still running, still fetching, invisible
to every later request, and stuck at the generation it was born with.

Every symptom follows: panels that never leave their skeletons, an applied
indicator that never clears, a stream that reports itself live because it is,
and not one line in any log, because nothing had gone wrong from any single
component's point of view.

The fix is to hold the lock across the whole get-or-create, including the store
and registry reads. That serialises surface creation, which is the intent rather
than a cost worth avoiding. A test runs two `for_surface` calls concurrently and
asserts both get the same `Arc`.

This would also have hit two people opening the same surface at the same moment,
and it is the kind of bug that no amount of reading finds. It took a browser,
frames printed from inside the page, and five runs.

**One change to the frontend for testability**, and only one: panels carry
`data-instance`, `data-panel`, `data-x/y/w/h` and `data-state`. A test can now
say "this panel moved" rather than parsing grid lines out of a style attribute,
and the state in the markup is the same word the stream uses rather than a
second naming of the same four states.

### 2026-09-07, eighth run — fifteen of fifteen

The last two failures were the tests, and both are better for the fix.

- **The rename test reloaded before the write landed**, so it was measuring how
  fast this machine is. It now waits for the `PUT` response. A person typing and
  reloading within a hundred milliseconds would lose the edit too; that is
  inherent to writing on blur and is not worth a guard today, but it is the
  reason this needed saying rather than a `waitForTimeout`.
- **The sparkline assertion matched three elements**, because the pack draws a
  sparkline per series rather than one chart with three lines. That is the
  pack's choice and a sensible one. The test now counts three and checks the
  table is gone, which says more than the original did.

**What the screenshots show.** Thirteen for composition and four for
degradation, and they were read rather than filed:

- The picker lists both platforms, six panels each, with the kind beside every
  name.
- Two panels from two platforms, drawn: a scalar with a delta, a timeseries with
  three coloured lines, a table with stages and timestamps.
- Dragging moves a panel and resizing grows it, both visibly.
- Switching a timeseries to `table` redraws the same data as time and three
  worker columns with units. The kind and envelope decoupling, working.
- A dead stream: "reconnecting" in red, then every panel unavailable saying
  "no answer" over "Hlin is not responding".

**One thing worth knowing that is not a defect.** Every layout write drops the
surface, so the next subscription rebuilds it from scratch and every panel
blanks for a moment. It is visible in the resize screenshot: a correctly resized
panel with an empty body, mid-refetch. It is correct and it is not free. Making
it smooth means diffing the written layout against the running surface and
touching only the panels that changed, which is a real piece of work and a
sensible thing to want once somebody is arranging a surface for more than a
minute at a time. Recorded rather than done.
