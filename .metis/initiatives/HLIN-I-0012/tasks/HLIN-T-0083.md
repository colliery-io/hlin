---
id: widgets-eighteen-to-twenty
level: task
title: "Widgets eighteen to twenty"
short_code: "HLIN-T-0083"
created_at: 2026-09-25T02:43:03.009423+00:00
updated_at: 2026-09-25T11:20:45.256028+00:00
parent: HLIN-I-0012
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0012
---

# Widgets eighteen to twenty

## Parent Initiative

[[HLIN-I-0012]]

## Objective

Task 4 of [[HLIN-I-0012]]: `deploys`, `converter`, `meetings`.

## Acceptance Criteria

## Acceptance Criteria

- [x] Three widget crates, following the rules below and [[HLIN-T-0080]]
- [x] `deploys` streams its log to its module over the bridge
      ([[HLIN-T-0068]]), the first real use of streaming
- [x] `converter` works entirely inside its module and makes no requests,
      and declares no fallback
- [ ] Each added to the list; "Twenty" now has twenty (these three are on the
      list; twenty once the other two widget tasks merge)
- [x] Server tests; the twenty flavour's browser test passes (with the six on
      this branch; with all twenty after the merge)
- [x] `angreal check all`, `angreal test all` pass

Every widget crate:
- lives at `crates/widgets/<name>/` with `src/` (the server) and `module/`
  (its Leptos module on `hlin-module`), both small and readable;
- uses `hlin-widget-support` for scaffolding and keeps its own rules and
  state in its own code;
- declares `ui`, `assets`, `routes` and `events`, and a shell-drawn fallback
  where a natural envelope exists (a counter as `stat`, a poll as `table`);
  a widget with nothing sensible to fall back to (the converter) declares
  none, and says so;
- is themed only through the `--hlin-*` tokens ([[HLIN-S-0007]]);
- announces `changed` after a write and publishes on its event stream, so a
  shared widget updates in every browser;
- has server tests for its rules, as the checklist and feed do.

## Implementation Notes

- Depends on [[HLIN-T-0080]] and [[HLIN-T-0068]].

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0012]] was decomposed. Not started.

### 2026-09-25 (implemented)

**What.**

- `deploys` (8218). A synthetic deploy log, the same for everybody: line `n`
  is a pure function of `n` (four lines to a deploy: started, built, rolling
  out, live or rolled back), due `n` ticks after the Unix epoch, so two
  browsers agree and a restart keeps history. `GET /api/deploys/log` is one
  long read of newline-delimited JSON: the last 12 lines at once, then each as
  it falls due (every second), written only as the connection takes it and
  never ending by itself; `?after=n` carries on after a line the reader has
  (clamped to the recent 12 and to now). A `Reading` guard counts open
  readers, which is how the tests see a reader's going reach the platform. No
  writes, so nothing to announce; not `pushed`. Fallback `table` of the recent
  lines, newest first, `refresh_ms` 5 s. The module follows the log with
  `fetch_stream` / `BodyReader`, puts lines back together across chunks
  (`Splitter`), keeps the newest 40 without repeats, shows *Live*, and on any
  end (the shell's `idle`/`rate`/`unreachable`, or a refusal in the
  platform's words) says why and follows again 3 s later from `?after=` its
  last line.
- `converter` (8219). Units, arithmetic and input all in the module
  (`module/src/units.rs`: length, mass, temperature, volume, speed, data, as
  affine maps onto a base unit; nothing below absolute zero). No routes of its
  own, state `()`, no fallback: a converter has no data to put in an
  envelope, so the manifest description and crate docs say so, and the shell
  saying the panel cannot be shown is the truth. What was typed survives the
  budget's unmount through `on_suspend` / `restored`. A server test holds the
  module to making no requests (its sources name no fetch, load or send).
- `meetings` (8220). Today's (UTC) meetings, a pure function of the date:
  a 09:30 stand-up and some of five more slots. Per person: answer going,
  maybe or declined to any of today's meetings that has not ended, and change
  it; nobody sees anyone else's. `PUT /api/meetings/{id}/answer`; announced.
  Fallback `table`, 60 s. The module shows times in the browser's zone and
  marks over / now / next from the browser's clock.
- `WIDGETS` gains the three; `twenty.spec.js` gains *the deploy log streams
  lines in as they happen, without a reload* (three more lines arrive on an
  untouched page, reloads counted).

**Streaming, first real use.** It worked with **no change** to `hlin-module`,
`hlin-widget-module` or `hlin-widget-support`: credit on consumption and
cancel on drop did what they say, and the shell's idle limit (60 s) is never
near with a line a second. What a second streaming widget would copy, and the
smallest additive fixes, not made here:

1. *Chunks are not lines.* Every NDJSON/SSE reader must reassemble lines
   (`Splitter` here). Proposal: `BodyReader::lines()` in `hlin-module`, an
   adaptor yielding whole `\n`-terminated lines.
2. *A refusal arrives as `Streaming` too.* A platform's 401/403 on a streamed
   read comes back as `StreamReply::Streaming { status: 4xx, .. }`, so to show
   it in the platform's words the module must gather the body and build an
   `Answer` itself. Proposal: `BodyReader::collect()` (or `StreamReply`
   returning a non-2xx as `Answered`).
3. *No follow-with-reconnect in the widget kit.* The loop (open, read, say why
   it ended, wait, reopen from where it got to) is 40 lines here. Proposal, if
   another widget streams: `hlin_widget_module::Widget::follow(request,
   on_chunk)`.
4. Minor: `EndError` is only reachable as `hlin_module::hlin_bridge::EndError`;
   re-exporting it beside `StreamError` would read better.

**Checks.** `cargo test` for the three: deploys 10 server + 3 module, converter
4 server + 8 module, meetings 10 server + 1 module. `angreal check all` clean.
`angreal test all` 1365 passed, 0 failed, 4 ignored (every `test result`
line summed, doc tests included). Against `demo up --with twenty --release`
(six widgets on this branch), `angreal e2e twenty` 3 passed: 6 ready in
~1.2 s; three new deploy lines in ~3.9 s on an untouched page. A throwaway
browser check also answered a meeting and converted 100 °C to 212 °F.

**Found, not fixed:** nothing in the shell or `hlin-ui`.
