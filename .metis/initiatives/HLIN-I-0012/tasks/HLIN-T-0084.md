---
id: measure-twenty-platforms-on-one
level: task
title: "Measure twenty platforms on one surface, and prove it degrades one at a time"
short_code: "HLIN-T-0084"
created_at: 2026-09-25T02:43:04.052973+00:00
updated_at: 2026-09-25T12:37:55.233140+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


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

## Acceptance Criteria

## Acceptance Criteria

- [ ] Browser tests on "Twenty": every widget reaches `ready`; scrolling top
      to bottom never has more than 12 frames mounted while none in view is
      ever unmounted; a scrolled-away widget with state gets it back on
      return (`suspend`/`state`) — *all but one clause: settled, never more
      than 12, but up to 18 in the document at the peak (see Results)*
- [ ] Kill one widget's process: its panel alone goes `stale` then
      `unavailable` (or falls back), and the other nineteen are unaffected;
      restart it and it recovers — *alone, yes, and it recovers; but an
      open page's panel never goes `stale`: it stays `ready` and the module
      says in words that it cannot reach its platform (see Results)*
- [x] A shared widget changed in one browser updates in another, and the
      time from write to the other browser is recorded
- [x] Numbers recorded in this task and in [[HLIN-I-0012]]: cold and warm
      time from navigation to every in-view widget `ready`; total bytes
      transferred cold and warm; JS heap and process memory with the surface
      open; mounted-frame count over a scroll. Measured three times, median
      reported, machine described
- [x] Compared against [[HLIN-S-0007]] NFR-1.1 (six modules interactive
      within 3 s cold), with a plain statement of whether twenty fits the
      bet, and what would have to change if not
- [x] Runnable by an angreal task

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

### 2026-09-25 — "ready" is not "something to look at"

With all twenty merged, `angreal e2e twenty` reports "20 widgets ready in
3622 ms" (release builds), and passes 6 of 6. But its screenshots taken at
that moment show the frames empty. A probe that screenshotted the surface at
intervals found the top six widgets fully drawn by 1.5 s, and every mounted
frame's DOM holding real content from 500 ms, so the widgets are fine. The
empty shots are the test photographing the instant the shell marks the last
panel `ready`, before the frames paint; the full-page shot does not composite
sandboxed frames reliably.

For this task that means two numbers, not one: time to every in-view widget's
`ready` (the handshake), and time to its first content (its DOM holding more
than "Loading…"). The second is what a person experiences. Screenshots should
be taken after first content, and the full-page shot replaced by scrolled
viewport shots.

### 2026-09-25 — measured

**How.** `angreal e2e twenty-measure` (new) runs `e2e/tests/twenty-measure.spec.js`
against `angreal demo up --with twenty --release`, and writes every number to
`e2e/measurements/twenty-<time>.json` (ignored by git). An init script in
every document records, as they happen, when each panel's `data-module`
becomes `ready` (the shell's page) and when each module's document first holds
more than "Loading…" (text outside `<style>`, or a control or a picture), so
how often the test polls does not change what is measured. Bytes are the
encoded body plus headers of every finished request, from Playwright (which
sees every frame), plus what open streams had received, from the page's
DevTools session. Memory is the resident set of every process descended from
the test worker, by Chromium process type, and the JS heap by
`Performance.getMetrics`. `angreal demo restart <widget>` (new) starts one
widget again exactly as `up` did. `angreal e2e twenty` now photographs after
first content, a viewport at a time (top, middle, bottom), instead of at
`ready` and full page.

**Machine.** Apple M3 Pro, 12 cores, 36 GB, macOS 26.6.2. Playwright 1.63,
its headless Chromium, one worker. Everything on loopback: the shell, twenty
widget processes and the browser on one machine. Another agent was building
on the same machine throughout (load average 3 to 6), so these are not
quiet-machine numbers. Viewport 1280×720 (Playwright's "Desktop Chrome",
which overrides the config's 1440×900), so **six** widgets are in view at the
top, not the eight or nine the layout was drawn for; nine frames are mounted
there (the three below are inside the 200 px mount margin).

Release WebAssembly for the frontend and every module; debug shell and widget
binaries (they serve bytes; see the first status update).

**Results.** Medians of three (the three runs agreed within 30 ms and a few KB).

| | Median |
|---|---|
| Cold, navigation → all six in view `ready` | **572 ms** |
| Cold, navigation → all six drawn (first content) | **588 ms** |
| Warm (same context, navigate again) → `ready` / drawn | 528 ms / 535 ms |
| Cold over 50 Mbit/s, 40 ms latency (DevTools throttling) → `ready` / drawn | 1,704 ms / 1,749 ms |
| Cold bytes, first screen (9 frames) | 7.73 MB: modules 5.69, shell frontend 2.02, platform requests 0.004, streams 0.005 |
| Warm bytes, first screen | 0.72 MB (0.69 MB of it one module's wasm fetched again, below) |
| Bytes to scroll the rest of the surface, down and back | 7.65 MB, all module assets |
| Every module asset once (20 modules) | 12.53 MB as sent; **3.98 MB gzipped** |
| Shell frontend | 2.01 MB as sent; 0.61 MB gzipped |
| JS heap, page and frames (one renderer) | 15.1 MB; 19.0 MB after a scroll |
| Browser resident memory, surface open | 526 MB (an empty page: 276 MB), renderer 264 MB (empty: 82 MB) |
| … after scrolling down and back (12 mounted) | 615 MB; renderer 347 MB |
| Frames mounted while scrolling, settled (1 s after each screenful) | **12** at most |
| Frames in the document while scrolling, at the peak | **18** |
| Frames unmounted while their panel was in view | none |
| Counter bump → the other browser (timed inside its frame) | 77 ms (85, 77, 77) |

Every module frame is in the page's renderer (one renderer process), so the
JS heap is one heap; the WebAssembly memory is not in it, which is why the
renderer's resident memory grows by 20 to 22 MB per mounted frame (the
shell's own page included in the share) while the heap grows by a few.

**NFR-1.1.** "A cached module mounts and says `ready` within 1 second; a
surface of six modules from three platforms is interactive within 3 seconds
cold." Six modules from six platforms: 0.59 s cold on loopback, 1.75 s cold
over a 50 Mbit/s link; 0.54 s warm. **Met, with room**, on this machine. Not
measured: a slower CPU, a real network with loss, or a phone.

**The claims.**

- *Scrolled away, state back.* The converter's typed value (its `suspend`
  hook) and the stopwatch running (kept by its platform) come back. The
  note's half-written draft was **lost**: the notes module had no `suspend`
  hook. Added one (`crates/widgets/notes/module`: the draft and the revision
  it started from, restored as a draft, with two unit tests); now kept.
- *Budget.* Settled, the page keeps to twelve. But the page stops counting a
  frame the moment it sends `suspend`, while the frame stays in the document
  until the module answers `state` or the 500 ms deadline passes (checked on
  a 250 ms tick). The SDK (`hlin-module`) sends nothing on `suspend` when a
  module has no hook, which is eighteen of the twenty, so each of those sits
  out the full deadline. Scrolling at 150 px per 150 ms that is up to 18 frames
  mounted at once, against "at most 12 frames are mounted" in HLIN-S-0007.
  Not fixed here (shell and SDK): either count suspending frames against the
  budget, or have the SDK answer `state` with no blob at once. The test
  asserts the settled count and records the peak.
- *One platform killed (dice, SIGKILL).* Nothing else moved: no other panel
  left `ready`, and the counter still bumps. But on the page already open,
  dice's panel **stays `ready`**: the heartbeat is between the page and the
  frame, and the module is alive. The module says "The widget could not be
  reached. Try again in a moment." in its own frame. A page opened while it
  is down gets `unavailable (unreachable)` at once and falls back to
  Hlin-drawn data (an empty table, data state `unavailable` once the
  registry has noticed). So "`stale` then `unavailable`" happens only to a
  frame mounted after the platform went away.
- *Recovery.* After `demo restart dice` nothing comes back by itself within
  20 s, as HLIN-S-0007 says (an unreachable module is remounted on its
  platform's next `changed` or when a person asks, not in a loop, and dice
  changes nothing until somebody rolls). "Try the module again" on the new
  page: `ready` and drawn in 0.85 s. A roll there is the platform's
  `changed`, which brought the open page's widget back in 0.25 s. The open
  page's widget offers no way to fetch again by itself: a failed read in
  `hlin-widget-module` is a refusal with no "try again" (only failed writes
  get one). Worth a follow-up.

**Compression.** Nothing is compressed. `/m/` answers `identity`: the shell's
asset proxy passes back only `ETag` and `Last-Modified`, and the widget
support crate compresses nothing anyway. The shell's own frontend is served
`identity` too (`ServeDir` with no compression). gzip would take module
assets from 12.53 MB to 3.98 MB and the frontend from 2.01 MB to 0.61 MB,
about 3.2 times less. Not changed here, as agreed. Also: every module's
`index.html` sets `data-wasm-opt="0"`, so release modules are not run
through `wasm-opt`.

**Warm is not free.** Each warm visit fetched one module's wasm again in full
(kanban's, 0.60 MB, in all three runs) while every other came from the
cache. Cause not established. Entry documents are `no-cache` and the widget
support crate sends no `ETag` for them, so every mount of an entry
re-downloads it (small: a few KB each).

**Does twenty fit the bet?** On time, yes: the first screen of a
twenty-widget surface is drawn in 0.6 s locally and 1.75 s over a good link,
well inside NFR-1.1, and a platform that dies takes only its own panel with
it. On weight, it fits only just, and the weight is the cost to watch.
Each module carries its own copy of Leptos and the SDK, 0.54 to 0.63 MB
(0.18 to 0.20 MB gzipped), and nothing can share it between frames; twenty
modules are 12.5 MB uncompressed, and each mounted frame costs about 22 MB of
renderer memory, so the budget of twelve is about 265 MB of renderer on top
of the page. What would have to change for this to be comfortable rather
than acceptable: compress what the shell serves (a 3× cut in bytes, and the
single largest win); run `wasm-opt`; make the budget honest about frames
being suspended; and for memory, nothing short of fewer mounted frames or
lighter modules. Not measured: the same widgets without frames, so what the
sandbox costs in time is not known. The six reach `ready` within 5 ms of each
other, which suggests the half second is the shell page's (its 2 MB frontend,
then the surface), not each frame's.
### 2026-09-25 — closed, with the defects tracked

Merged on main; 1003 Rust tests pass. The two criteria left partly met are
carried by their own tasks rather than held open here: the budget peak of 18
by [[HLIN-T-0085]], and a killed platform's panel staying `ready` by
[[HLIN-T-0087]]. Compression and `wasm-opt` are [[HLIN-T-0086]]; the warm
re-download is [[HLIN-T-0088]].
