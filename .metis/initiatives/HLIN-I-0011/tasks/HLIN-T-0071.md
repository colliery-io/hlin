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

- [x] A small Leptos module on `hlin-sample-platform`, built with the SDK:
      follows the time picker, makes a read and a write as the viewer, and
      announces `changed`
- [x] The demo builds it (`angreal ui build` or its own task) and `demo up`
      serves it
- [x] Browser tests that it draws, follows context, and that a write by one
      viewer reaches another's module
- [x] Containment matrix, settled and written into [[HLIN-S-0007]], asserting
      a hostile test module cannot: read the parent document, read cookies or
      storage, fetch the network, navigate its frame off `/m/`, call `/p/`
      directly, reach another platform's prefix, flood messages past the rate,
      or starve the page with streams
- [x] Load time measured: a cached module `ready` within 1 s; six modules from
      three platforms interactive within 3 s cold. Recorded in the task and
      failing the test if missed
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass (the
      first two do; `e2e test` has one failure, in `streams.spec.js`, whose
      cause is in the shell and is recorded below)

## Implementation Notes

- Depends on everything else in [[HLIN-I-0011]].
- Mind the `e2e-frontend-must-match` memory: the demo flavour the tests expect
  must be the one `demo up` ran.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-25 (implemented; one e2e failure outstanding, not this task's code)

**What.**

- *The annotations module* (`crates/hlin-sample-platform/module`, package
  `hlin-sample-platform-module`): Leptos on `hlin-module`, no bridge code of
  its own. Reads `GET /api/module/annotations?from=` for the surface's range
  (a range ending within a minute of now is read open-ended, since the shell
  resolves "the last hour" once and a note pinned later would fall just
  after it), shows who the platform says is asking, pins a note with
  `POST /api/module/annotations/pin` as an `Attempt`, says `changed` after a
  write and reads again either way. Stamps `data-drawn-at` on its body at
  first content, for the load-time test. Trunk setup, `boot.js` loader and
  hashed output as the checklist's.
- *Platform*: `src/annotations.rs` (in-memory, bounded, idempotent by key),
  the two routes (the write verifies a token bound to `POST` and the path),
  `src/built.rs` serving the build under `/ui/annotations/` from
  `--module-dir`, a write prefix `/api/module/annotations/`, contract 2.4.0,
  panels `annotations` and `module-hostile`. The first cut declared the write
  prefix without its trailing `/`, which the manifest crate reports only as
  an unusable route, so every write was refused `outside_prefix`; the
  reference manifest test now asserts no unusable routes.
- *Hostile module* (`ui/hostile`, plain JS) and `ui/lure` (a page and script
  that must never load). Every attempt runs in a task of its own: Chromium
  lets `eval` run inside a debugger's evaluation whatever the CSP says, so an
  attempt made inside Playwright's `evaluate` call proved nothing.
- *Demo*: a third sample platform, `smelter` on 8085 (in all four sample
  configurations), so six modules can come from three platforms; `demo up`
  builds the module before starting the sample platforms and passes
  `--module-dir`; `_wait_for_panels` waits for every sample platform;
  `demo/state/build.json` records the flavour and whether it was
  `--release`.
- *Spec*: [[HLIN-S-0007]] *Containment* (the matrix, per-browser findings,
  what is recorded rather than prevented); the open question replaced by
  one about CPU isolation outside Chromium.

**Browser tests.** `annotations.spec.js` (3): draws and reads as
Development User; follows the picker (60 → 15 → 1440 minutes); a pin in one
browser appears in another without a reload, and a refusal is shown in the
platform's words. `containment.spec.js` (18): the matrix in the spec.
`performance.spec.js` (2): NFR-1.1. `surface.spec.js` now expects three
platforms. `HLIN_BROWSERS=chromium,chromium-full,firefox,webkit` selects
projects; the default is Chromium alone.

**Containment, as run** (Chromium 153 headless shell and full, Firefox 155,
WebKit 26.6): every row holds in all four. Differences are the browsers':
a spinning module froze the whole page for its full 5 s in the headless
shell, Firefox and WebKit (4–5 frames drawn in 3 s; the page's heartbeat
clock frozen too, so no `stale`), while full Chromium isolated it (179–181
frames, longest gap 17 ms, `stale` then `ready`); clipboard writes are not
withheld by `allow=""` in Firefox or WebKit; a blocked navigation leaves an
error page in Chromium and Firefox and is cancelled in WebKit; Playwright
cannot take the CSP away in WebKit, so the direct-`/p/` row's second half
is skipped there. No hole in the shell was found.

**Load time (NFR-1.1)**, `angreal demo up --release`, standard flavour,
medians of three runs, times in ms:

| Browser | Cached module: `ready` / first content, from its frame going in | Six modules, three platforms, cold: last `ready` / last first content, from navigation |
|---|---|---|
| Chromium (headless shell) | 482 / 492 | 556 / 561 |
| Chromium (full) | 481 / 488 | 529 / 537 |
| Firefox | 497 / 505 | 573 / 578 |
| WebKit | 486 / 494 | 544 / 552 |

Both limits met with room; the test asserts them (1 s, 3 s) on any build
(debug: 433/441 and 602/611). Nearly all of the cached ~480 ms is the
page's `init` cadence, not the module: every asset is in hand 5 ms after the
frame goes in, but the `init` sent on `load` arrives before a WebAssembly
module is listening, and the resend is on a 250 ms clock checked every
250 ms, so it lands 250–500 ms later. A module saying it is listening, or a
resend on the frame's first message, would take a cached module to tens of
milliseconds; not done here.

**Outstanding: `streams.spec.js`, "a module that stops pulling…".** With the
new specs running before it, the held stream is ended `rate` by the shell
while the module holds it, although the feed is paced well under
`stream_bytes_per_second`. `/m/` and `/p/` share the shell's `proxying`
client, so the multi-megabyte module downloads warm the pooled connection
to the platform; with its buffers grown, several megabytes back up behind a
held stream, and when the connection to the browser takes more the shell
reads that backlog in one go, which the one-second bucket counts as the
platform exceeding its rate (HLIN-T-0068 recorded the trade-off assuming
the backlog would be small). The test passes alone and in the suite
without the new specs. The test itself was also fixed for a second fault:
one 64 KiB `pull` was not enough to reach back through this machine's
buffers (it failed 3 of 3 alone); it now grants credit a little at a time
until the platform writes, with a lighter feed. The shell fix (measure the
platform's pace rather than the shell's catch-up, or give streams their own
unpooled connections) is a follow-up for [[HLIN-S-0007]] *Streaming*.

**Checks.** `angreal check all` clean; `angreal test all` 1013 passed, 0
failed, 3 ignored; `angreal ui build` ok; `angreal e2e test` (`--with
aurora`) 71 passed, 1 failed (above), 24 skipped, 4 not run after it;
`angreal e2e signin` (`--with collab`) 9 passed. The dev database's contract
memory for orebank, stampmill and smelter was cleared afterwards, so a demo
of `main` (contract 2.3.0) is not flagged as a breaking change.
