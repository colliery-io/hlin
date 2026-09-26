---
id: prove-and-measure-the
level: task
title: "Prove and measure the containerised twenty"
short_code: "HLIN-T-0095"
created_at: 2026-09-25T23:55:00.609203+00:00
updated_at: 2026-09-26T03:10:00.000000+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
initiative_id: HLIN-I-0013
---

# Prove and measure the containerised twenty

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Run the twenty browser suite and the twenty measurement against the
containers, signed in through Dex, and record the numbers beside the
process-based ones in [[HLIN-I-0012]].

## Acceptance Criteria

## Acceptance Criteria

- [x] `angreal e2e twenty` and `twenty-measure` (or container-aware variants)
      pass against the compose deployment, signed in through Dex
- [x] The kill-and-recover check uses `docker compose stop`/`start` on a
      widget container, twice in one session
- [x] Each widget's core UI is reachable on the compose network at `/`, and
      an unknown `/hlin/*` path answers 404 (checked from inside the
      network)
- [x] Numbers recorded (cold and warm first content, bytes, memory, peak
      frames), medians of 3, compared with [[HLIN-I-0012]]'s, with any
      difference explained
- [x] README: how to run the containerised demo, and that it is the reference
      for how a platform should serve Hlin

## Implementation Notes

- Depends on [[HLIN-T-0094]].

## Status Updates

### 2026-09-25

Created. Not started.

### 2026-09-26 — the twenty suites against the containers

- **One suite, two deployments.** `twenty.spec.js` and
  `twenty-measure.spec.js` are parameterised rather than copied:
  `e2e/tests/twenty-deployment.js` says, for `process` or `compose`
  (`HLIN_TWENTY`), where a widget's own UI is (`127.0.0.1:82xx` or
  `http://<name>.comp.test:8080`), how a context reaches it, and how a
  platform goes down and comes back (SIGKILL and `demo restart`, or `docker
  compose stop` and `start`, then the healthcheck's request answered inside
  the container). The shell's address is `HLIN_URL`, as before.
- **Signed in through Dex.** `e2e/global-setup.js` signs in once through
  Dex's own form (`HLIN_SIGN_IN=alice@example.com`) with the sign-in helper,
  now one `signIn` in `helpers.js` used by collab, collab-modules and
  walkthrough too (it waits to be anywhere but `/dex`, not on port 8080).
  The session's cookies become every context's `storageState`, with
  `ignoreHTTPSErrors` for the throwaway CA; the cache is not carried, so a
  cold context is still cold.
- **Own UIs from inside the network.** Nothing publishes a widget, so
  `angreal e2e twenty --against compose` runs `e2e/network-proxy.js` in a
  `node:22-alpine` container on `hlin-twenty_comp`, published on a loopback
  port Docker picks, for the length of the suite, and removes it after. It
  forwards (or tunnels with CONNECT, which Playwright's request client uses)
  only to `*.comp.test`. The browser opens `http://clock.comp.test:8080/` by
  its name; `/`, `/nope` (200, its UI) and `/hlin/nope` (404, no `<title>`)
  are checked for all twenty through it, i.e. from the compose network.
- **angreal.** `e2e twenty` and `e2e twenty-measure` take `--against
  process|compose` (default `process`). Against `compose` they refuse unless
  the `hlin-twenty` shell container is running and 127.0.0.1:8090 answers
  with a sign-in; against `process`, with nothing on 8080 but the containers
  up, they say to add `--against compose`; anything else for `--against` is
  refused by name.
- **Twice in one session.** The kill-and-recover test now takes the platform
  away and back twice on the same open page (both deployments), each round
  with its own page opened while it is down.
- **Found: `docker compose stop` took ten seconds.** No widget server (nor
  the shell) handles SIGTERM, and as PID 1 a process is not killed by a
  signal it has no handler for, so `stop` waited out its grace period and
  sent SIGKILL: the first run's panel went `stale` 10.4 s after the stop
  began. `init: true` on every widget and the shell in
  `deploy/twenty/compose.yml` puts Docker's init in front, which passes
  SIGTERM on; `stop` now takes 0.2 s. The test records how long the stop
  took (`stopTookMs`).

**Results, against compose** (`angreal e2e twenty --against compose`): 9 of 9
passed, twice. `angreal e2e twenty-measure --against compose`: 4 of 4
passed, twice; the numbers below are the second run (after `init: true`),
same machine (M3 Pro, 12 cores, 36 GB), headless Chromium, Playwright 1.63,
1280×720, six in view, medians of 3. Beside them, [[HLIN-I-0012]]'s
process-based numbers with the shell compressing once ([[HLIN-T-0091]];
heap and memory are [[HLIN-T-0084]]'s, which compression does not move, and
the bump and recovery [[HLIN-T-0087]]'s):

| | Processes (`--with twenty --release`) | Containers (`--with twenty-compose`) |
|---|---|---|
| Cold, navigation → six `ready` / drawn, loopback | — / 599 ms | 590 / 603 ms |
| Cold over 50 Mbit/s, 40 ms → six drawn | 1,091 ms | 1,076 ms |
| Warm → six drawn | 534 ms | 533 ms |
| Cold bytes, first screen | 1.95 MB | 1.94 MB (modules 1.47, shell 0.45) |
| Every module asset once, as sent | 1.83 MB | 1.93 MB with the shell's cache warm; 2.36 MB on a just-started shell |
| Warm bytes, first screen | 0.05 MB | 0.05 MB |
| JS heap | 15.1 MB | 15.6 MB (12.6 after a scroll) |
| Browser resident memory | 526 MB (615 after a scroll) | 525 MB (636 after a scroll); 277 MB empty |
| Frames mounted while scrolling, peak / settled | 12 / 12 | 12 / 12 |
| A counter bump, one browser to another | 77 ms | 93 ms (81–98) |
| Platform down → panel `stale` (roll, *Try again*) | 0.19 s | 0.38 s / 0.32 s (0.2 s of it `stop`) |
| Platform back → open page `ready`, drawn | 4.8 s | 4.8 s / 3.8 s |

**Why they are the same, and where they are not.** Both are a release
shell compressing each file once, on one machine; what a browser pays is
the shell's answers, and the containers change only the path to them. The
published 8090 goes through Docker Desktop's port forwarding into its Linux
VM, and the shell reaches a widget over the compose bridge rather than
loopback; at these sizes neither shows beyond the runs' spread in the first
screen's time or bytes. Where it does show: a change's trip (widget →
shell's stream → browser) crosses both, and is about 15 ms longer; stopping
a container is an operator's `stop` (0.2 s) where the process was SIGKILLed
at once, so `stale` comes that much later. "Every module asset once" is
larger on a just-started shell because its compressed-once cache is empty
and answers each file's first request at brotli 4 ([[HLIN-T-0091]]); the
first measurement, on a shell that had already served everything, sent
1.93 MB, 0.1 MB over the processes' 1.83: files whose upgrade to brotli 11
had not finished when they were asked for, which is a matter of when the
shell last started, not of containers. Recovery is the same by design: the shell's subscription coming
back is the signal, in either deployment. Memory is the browser's, and the
same surface draws the same frames.

`angreal check all` clean; `angreal test all` 1085 passed, 3 ignored.
