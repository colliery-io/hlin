---
id: give-modules-the-surface-s-context
level: task
title: "Give modules the surface's context, relay what changed, and keep a surface within its frame budget"
short_code: "HLIN-T-0067"
created_at: 2026-09-25T00:01:05.391426+00:00
updated_at: 2026-09-25T01:36:20.769316+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Give modules the surface's context, relay what changed, and keep a surface within its frame budget

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 7 of [[HLIN-I-0011]]. [[HLIN-S-0007]] `context`, `theme`,
`visibility`, `changed` (both ways), `set-param`, `set-range`, `navigate`,
`notice`, and *Budget*.

## Acceptance Criteria

## Acceptance Criteria

- [x] `context` sent on every time-range or parameter change, carrying only
      declared parameters; `theme` on scheme or token change; `visibility`
      on scroll and tab visibility
- [x] `set-param` and `set-range` have exactly the effect of the chrome's
      controls and `Intent::Select`/`Intent::Range`, stored with the layout
- [x] `changed` from a platform's event stream reaches that platform's mounted
      modules; a module's `changed` is relayed with `from: "module"` to that
      platform's other modules on every surface this shell serves (needs a
      shell-side relay, not only the page)
- [x] `navigate` opens a panel or page; unknown targets ignored and logged
      (a panel: yes. A page: logged and ignored until the shell hosts pages,
      HLIN-T-0070)
- [x] `notice`: plain text, at most 140 characters, labelled as the
      platform's, in that panel's frame only
- [x] Budget: at most 12 mounted; the least recently seen out-of-view frame is
      unmounted first; never one in view; `suspend` then `state` (at most
      `state_bytes`, 500 ms deadline) kept in page memory and returned as
      `init.restored`
- [x] Tests for each, browser tests for budget and relay across two browser
      contexts
- [x] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0066]].
- The shell already follows each platform's event stream
  (`crates/hlin/src/stream/streams.rs`); the relay to modules needs a path
  from there, and from `/p/`-carried module `changed` messages, to the browser.
  The existing surface stream is the obvious carrier.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-24 (started)

Read the task, HLIN-S-0007, HLIN-T-0066's notes, the frame host, the shell's
stream and event plumbing. Plan:

- **Relay through the shell.** The page cannot reach another person's browser,
  so a module's `changed` is posted by the page to
  `POST /api/stream/{surface}/changed` (same-origin, `Author`, the layout
  visible to the caller and holding a module panel of that platform). The
  shell puts it on one shell-wide broadcast beside the platform event streams
  (`Streams::relay`). Every running surface drains it each tick: it nudges its
  own shell-drawn instances exactly as a platform event would, and, where its
  layout holds a module of that platform, sends a new `changed` frame down the
  existing surface stream (HLIN-S-0003, additive). Platform events a surface
  already follows become the same frame with `from: "platform"`; a surface now
  follows the events of every platform whose *module* is on it, not only of
  pushed panels. The page turns the frame into bridge `changed` for its
  mounted modules of that platform, skipping the frame it relayed itself (it
  delivered those locally at once) by a per-page id.
- **Page.** Pure rules in `bridge.rs` (which modules hear a change, the
  budget's choice, notice text, the scheme from the pack's tokens, which
  parameters a panel may set); wiring in `frame.rs`: `context` diffed per
  frame, `theme` from the pack's `--hlin-*` tokens, `set-param`/`set-range`
  through the same handler as `Intent::Select`/`Intent::Range`, `navigate`,
  `notice`, and the budget with `suspend`/`state`/`init.restored`.
- **Sample platform.** A `module-context` panel (the probe, declaring
  `time_range` and `cluster`), and the probe grown to show context, theme,
  changes and notices, and to keep a note across `suspend`.

### 2026-09-24 (implemented)

**What.** As planned.

- *Stream (HLIN-S-0003, amended additively).* A `changed` frame
  (`platform`, `panel`, `selections`, `from`, optional `page`) as its own SSE
  event name, and `POST /api/stream/{surface}/changed` (`ChangedRequest`).
- *Shell.* `Streams` carries a shell-wide broadcast (`relay`/`relayed`) beside
  the per-platform subscriptions. `modules/changes.rs` is the endpoint:
  same-origin as a write (`check_caller`), `Author` (so `401 not_signed_in`,
  `403 read_only`), 16 KiB, and `404` unless the layout, as the caller sees
  it, holds a module of that platform. Each `LiveSurface` knows which
  platforms' modules its layout holds (`Composition.modules`, refreshed on a
  layout write), follows those platforms' events too (not only pushed
  panels), re-follows when the layout is written, and each tick drains both
  roads: a module change nudges its shell-drawn instances exactly as a
  platform event does, and either becomes a `changed` frame where the surface
  shows that platform's modules. `crates/hlin/tests/relay.rs`: 6 tests.
- *Page.* `bridge.rs` gained `declared_only`, `hears`, `notice_text`,
  `THEME_TOKENS`/`scheme_of`, `over_budget`/`FRAME_BUDGET`, `keeps_state`
  (9 more host tests). `frame.rs`: `context` diffed per frame on every
  `set_context`, and sent on `ready` if it moved since `init`; `theme` built
  from the computed `--hlin-*` roles and sent on `init`, `ready` and a
  `prefers-color-scheme` change; `set-param` (declared only) and `set-range`
  handed to the app as `hlin_view::Intent`s through the same `act` the
  chrome's controls and components use; `navigate`; `notice`; `changed` both
  ways; the budget. `app.rs`: `act` shared, `asked_range` (the range and
  generation the shell was last asked for) drives `context`, `navigate`
  scrolls to the panel, the notice is drawn above the frame.
- *Sample platform 2.3.0.* `module-context` (probe; `time_range`, `cluster`);
  the probe shows range/params/generation, theme, visibility, changes (per
  source), restored state, and has buttons for each request.
- *Browser.* `e2e/tests/bridge.spec.js`, 9 tests: context follows the picker
  (declared params only); theme matches the page's tokens; `set-param` stored
  and surviving a reload, undeclared refused; `set-range` moves the picker;
  notice; navigate and an unknown target; budget of 13 (first unmounted,
  `#visible` no/yes, remount with `init.restored`); relay across two browser
  contexts on two layouts (the reader refetches, the writer's neighbour hears
  it locally, the writer does not hear itself); a platform event reaching a
  module.

**Found.**

- *A panel far down a surface would have been given up on before it was ever
  mounted.* `Liveness` was created when the panel was hosted, and the tick
  judged it with no frame: ten seconds later it was `unavailable
  (unreachable)`. Liveness now starts at `attach`, and the tick skips panels
  with no frame. The budget test is what would have shown it.
- *Module-only panels are in no aggregator instance*, so a surface did not
  follow their platform's events at all. Hence `Composition.modules`.

**Decisions.**

- *Relay through the shell, delivered on the surface stream.* The page tells
  its own other modules at once, and posts; its echo is skipped by a random
  per-page id carried in the frame. Two tabs of one person on one layout share
  a `LiveSurface`, so an instance id could not tell them apart; a page id can.
- *Who hears a change:* every running module of the platform, whichever panel
  it draws (the message names the panel; a module refetches if it cares),
  except one that selected a different value for a parameter the change names.
- *Theme tokens are the chrome's eleven `--hlin-*` roles*, as the mounted pack
  filled them (computed style), not a new vocabulary. The scheme is read from
  `--hlin-surface`'s luminance, the system preference only when unreadable,
  because the pack decides (Aurora Dark is dark in a light browser).
- *`context` is driven by what the shell was last asked* (`apply_range`),
  not by the picker signals, so a module and the panels beside it answer the
  same range and generation.
- *`navigate` to a panel* brings the instance on this surface into view (one
  other than the asker's own if there are several) and marks it for 1.5 s. A
  panel not offered, not on the surface, or any `page` target is ignored and
  logged on the console. Adding a panel to the layout was rejected: a
  module should not be able to edit a person's layout.
- *Notice:* control characters and whitespace runs collapse to one space; over
  140 characters is cut to 139 plus `…`; drawn as text, as "{Platform} says",
  above the frame; an empty notice clears it; replaced on remount.
- *Budget:* out-of-view frames, those not within the mounting margin first,
  then least recently seen. A frame that can hear gets `suspend` and goes on
  `state` or after 500 ms; one still loading goes at once; one that comes back
  into view while suspending stays. A blob over `state_bytes` is dropped; an
  unsolicited `state` is ignored; the kept blob goes in every `init` until
  `ready`, then is forgotten; removing the panel forgets it.
- *Remount on a platform `changed`* (HLIN-T-0066's open item): a module given
  up on as `unreachable` is remounted when its platform's event names its
  panel key. Not on any event from the platform: the sample platform reports
  every few seconds, and a wedged or silent module would be remounted in a
  loop.
- *Messages before `ready`* other than heartbeat, fetch and state are ignored.
- The probe refetches only on news about its own panel, so the flood test is
  not disturbed by the platform's own events.

**Checks.** `angreal check all` clean; `angreal test all` 689 passed, 0 failed,
3 ignored; `angreal ui build` ok. Against `demo up --with aurora`, `angreal e2e
test`: 39 passed, 11 skipped, 0 failed. Against `--with collab`, `angreal e2e
signin`: 5 passed.

**For the next tasks.** HLIN-T-0068: `handle` still drops `pull`/`cancel`;
unmounting (by the budget's `detach` too) must end a frame's streams with
`unmounted`; streams are allowed while hidden, and a suspended frame is
detached, so count streams per mounting (`serial`). HLIN-T-0070: `navigate`
with a `page` target is logged and ignored in `app.rs::navigate`; the budget
counts only panel frames (`Host`), and a page frame should never be unmounted
by it; `init.page` and no `restored` for pages.
