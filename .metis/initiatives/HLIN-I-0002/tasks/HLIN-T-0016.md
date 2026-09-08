---
id: demo-walkthrough-one-command-up
level: task
title: "Demo walkthrough: one command up, one command proves it"
short_code: "HLIN-T-0016"
created_at: 2026-09-07T14:04:27.130466+00:00
updated_at: 2026-09-07T17:20:57.168270+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Demo walkthrough: one command up, one command proves it

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Make the demo reproducible by anyone with the repository: one command brings everything up, one command proves the degradation cases without a person watching. This is the initiative's exit criterion made executable, and it is what a new platform team runs on their first day.

## Acceptance Criteria

- [x] An angreal `demo` group: `up` starts the database (via the `db` tasks), builds the UI, starts two sample platforms and the shell as managed background processes with logs under a known directory, waits for each to answer, and prints the URL; `down` stops them all; `status` shows what is running
- [x] A checked-in demo configuration: `demo/hlin.toml` naming the two platforms, and the key path under a gitignored `demo/state/`
- [x] `demo walkthrough` scripts and asserts, against the running demo: both platforms listed by `/api/platforms` with their panels accepted; a stream opened for the default layout delivers `ready` frames for panels from both platforms; changing the time range advances the generation and yields new frames; stopping platform B produces `unavailable(unreachable)` frames for its panels while platform A's stay `ready`; restarting B with `--breaking` produces a violation line in the shell log naming the removed key and the expected major, and the removed panel becomes `unavailable(unknown)`; restarting B normally restores it
- [x] The walkthrough exits non-zero with a specific message on the first assertion that fails, so a regression is named rather than discovered
- [x] A "Try the demo" section in the README: prerequisites (Rust, Docker, `trunk`), the two commands, what to expect in the browser, and how to run the walkthrough
- [x] The whole thing passes from a clean checkout on a machine with the prerequisites, and `angreal demo down` leaves no processes behind — including the container path, once there was disk for it. See the second status update

## Implementation Notes

### Technical Approach
Managed background processes need PIDs and logs; write a small process registry under `demo/state/` so `down` and `status` know what `up` started. The walkthrough is a Python task using `urllib` and a line-by-line SSE reader; keep it dependency-free so it runs wherever angreal does. Assertions should read the shell's log file for the violation line rather than depending on stdout capture.

### Dependencies
Everything: [[HLIN-T-0009]] through [[HLIN-T-0015]].

### Risk Considerations
Readiness detection is where demo scripts usually flake. Poll each process's health endpoint with a bounded wait rather than sleeping for a fixed time, and make the timeouts generous on first run when the UI build is cold.

## Status Updates

### 2026-09-07 — the demo runs, and proves itself

Two angreal groups' worth of work in one: `demo up`, `status`, `down` and
`walkthrough`.

**A defect the walkthrough was written to find, and found.** The criterion asked
that a panel withdrawn by a platform become `unavailable(unknown)`.
`Surface::set_registry_state` existed to do exactly that, had a test, and was
**called from nowhere**. The registry noticed a panel disappearing; the running
surface never heard about it, went on asking for an endpoint the current
manifest no longer declared, and would have reported the resulting 404 as
`malformed` — a defect blamed on the platform rather than a panel that was
withdrawn. `LiveSurface::reconcile` now runs once a second beside the staleness
check and brings the surface into line with what the registry accepts.

Its one restraint is the important part: it only concludes anything about a
platform it has actually heard from. An unreachable platform has an empty panel
list, and reading that as "it withdrew everything" would turn a network problem
into a contract one, which is the exact confusion the state machine exists to
prevent. Four tests pin the behaviour, including that saying a panel is gone
twice sends one frame, because a check running every second must not send a
frame every second.

A panel that comes back goes to `loading`, not to the data it held before it
went away. A number from before the platform changed its mind is not a current
number.

**Processes, not containers.** `up` starts ordinary background processes and
writes a registry under `demo/state/`, which `down` and `status` read. Putting
the platforms in containers would hide the thing the demo exists to show: the
shell is one binary, the platforms are theirs, and nothing between them is doing
any work. Readiness is polled against each health endpoint with a bounded wait,
never slept through, because a fixed sleep is either flaky on a cold machine or
wasted on a warm one.

**Two things that had to be learned by running it.** Calling a decorated angreal
command from Python calls the framework's wrapper, not the work: `up` calling
`down` hung with no output until both bodies were pulled out into plain
functions. And angreal shows a task's stdout when the task succeeds and swallows
it when it does not, which is exactly backwards for a script whose job is to
name what went wrong — the walkthrough's first failing run exited 1 and printed
nothing at all. Everything it says now goes to stderr through one `say`.

**The database is a convenience, not a requirement.** `up` checks whether
something is already listening on 55432 and leaves it alone if so, and `down`
only stops the container it started. The shell connects to a URL and applies its
own migrations; nobody running their own Postgres should be made to start a
second one.

**What was verified.** The full sequence, twice:

- `angreal demo up` from nothing: database, frontend, both platforms, the shell,
  each waited for, URL printed.
- `angreal demo walkthrough`: all eight steps passed, exit 0. Both platforms
  discovered with six panels each, a surface composed from both, both panels
  `ready` with data, the generation advancing on a time-range change,
  `stampmill` killed and its panel `unavailable(unreachable)` while `orebank`
  stayed `ready`, `stampmill` back with a breaking manifest producing the
  violation line naming `queue-depth` and expecting `2.0.0`, the panel reading
  "this panel is no longer offered", and the panel `ready` again after a normal
  restart — with no restart of the shell.
- The failure path, on purpose: with the control platform stopped, the
  walkthrough exited 1 having said "orebank should be reachable; the shell is
  polling it every ten seconds".
- `angreal demo down`: all three processes stopped, the registry removed, no
  `hlin` processes left.

Checks clean, 263 tests passing.

**Two gaps.**

*Not from a clean checkout.* Docker's virtual machine on this machine is out of
disk — `initdb` fails with "No space left on device" — so `angreal db up` cannot
run here. The demo was verified against a Postgres started by hand on the same
port, which is what the "already listening" path exists for, but the container
path in `up` is unexercised. Nothing was pruned to make room; that is the user's
disk to manage. **Closed the same day; see below.**

*Still nobody has looked at it.* The Chrome extension is not connected, so no
browser has rendered the frontend across three tasks now. The picker, the drag,
the resize handle and the pointer-capture behaviour are unit-tested underneath
and unexercised on top. The README's "Try the demo" tells a person what they
should see; nobody has checked that they see it. It is the one claim in this
initiative resting on reasoning rather than observation, and it is a claim
about the visible surface of the product, which is the worst place to have one.

### 2026-09-07, later — the container path, verified

The user asked for a Docker clean, which freed 15.5GB and made the first of the
two gaps above closable. `docker system prune` without `-a` and without
`--volumes`: stopped containers, dangling images, unused networks and build
cache. Images that are merely unreferenced and every volume were left alone,
because those hold work somebody may want and a request to clean up is not a
request to throw away data. All eleven of the machine's running containers
stayed up throughout.

With disk available, the whole sequence was run again on the containerised
database rather than one started by hand:

- `angreal db up` started the container, waited for it to accept connections,
  and reported ready. This is the command that previously failed with
  "No space left on device".
- `angreal demo up` brought up the frontend, both platforms and the shell
  against it.
- `angreal demo walkthrough` passed all eight steps, exit 0.

The layout the walkthrough composed carried a new identifier, which is worth
noting: the database was empty, so this also exercised the first-visit path
creating a layout in a fresh store, rather than reusing one left by an earlier
run. The `[~]` on the last acceptance criterion is now `[x]`.

The other gap stands. Nobody has looked at the frontend in a browser.
