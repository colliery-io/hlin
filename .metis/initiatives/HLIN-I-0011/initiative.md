---
id: platforms-ship-ui-modules-and-the
level: initiative
title: "Platforms ship UI modules, and the shell hosts them in sandboxes"
short_code: "HLIN-I-0011"
created_at: 2026-09-24T22:23:21.385709+00:00
updated_at: 2026-09-24T22:23:21.385709+00:00
parent: HLIN-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/discovery"


exit_criteria_met: false
estimated_complexity: XL
initiative_id: platforms-ship-ui-modules-and-the
---

# Platforms ship UI modules, and the shell hosts them in sandboxes

## Context

[[HLIN-A-0014]] changed what Hlin is: the place people work, with each
platform shipping its own UI as modules, and the shell running every module in
a sandboxed iframe while owning the page, the person and the wire. The vision
was amended the same day ([[HLIN-V-0001]], Amendments, 2026-09-24).
[[HLIN-A-0013]] settled how a module's requests reach its platform.

None of it exists yet. The shell today draws every panel itself from data, and
nothing it serves runs a platform's code. This initiative builds the module
host: everything the shell needs so that a platform can ship a Leptos module
and have it run, isolated, on a surface, as the person looking at it.

[[HLIN-I-0010]] (the checklist and feed demo) is its first real use and
follows it.

## Goals & Non-Goals

**Goals:**
- A panel or a page can declare a module, and the shell runs it in a sandboxed
  frame on a surface.
- A module can do everything [[HLIN-A-0014]] promises, and nothing it rules
  out: receive context and theme, make requests to its own platform under
  declared prefixes with the viewer's identity, set shell parameters, navigate,
  and announce changes.
- Isolation is proven, not assumed: browser tests that a module cannot read the
  shell's page or cookies, cannot reach the network, cannot reach another
  platform, and cannot take the page down.
- Rendering stays total: a module that never loads, never says `ready`, or
  stops answering is a panel in a known state, and falls back to shell-drawn
  data where the panel declares it.
- A Rust SDK so a Leptos module does not know it is in a frame.
- Load time per surface measured and bounded, since weight is the new bet's
  first risk.

**Non-Goals:**
- The checklist and feed platforms, Dex, and the two-person demo. Those are
  [[HLIN-I-0010]].
- Retiring the shell-drawn path. Envelopes, kinds, packs and the stream stay.
- Platform CSS beyond what a module ships inside its own frame, streaming
  `fetch`, and kit-version enforcement. These are open questions below, not
  goals.
- Migrating any real platform's frontend.

## Architecture

Decided in [[HLIN-A-0014]] and [[HLIN-A-0013]]; summarised here so the slices
below have something to point at.

```
browser                                        shell                    platform
┌──────────────────────────────────────┐
│ shell page (the only shell-origin    │
│ code): grid, chrome, frame states    │
│   ┌───────────────┐   postMessage    │   /p/{platform}/{path}
│   │ module frame  │ ───────────────▶ │ ───────────────────────▶ prefix check,
│   │ sandbox,      │ ◀─────────────── │ ◀─────────────────────── bound token ──▶ decides
│   │ opaque origin │   bridge         │
│   └───────────────┘                  │   /m/{platform}/…  assets, CSP, cache
└──────────────────────────────────────┘
```

- **Frame:** `sandbox="allow-scripts allow-forms"`, no `allow-same-origin`.
  Assets from `/m/{platform}/…` on the shell's origin, proxied from the
  platform's declared asset prefix, cached by content hash, served with a CSP
  that confines scripts, `wasm-unsafe-eval` and connections to the module's
  own assets.
- **Bridge:** a versioned `postMessage` protocol. The page accepts a message
  only from a frame it created, and knows platform, panel and instance from
  that alone.
- **Requests:** `/p/{platform}/{path}` from the shell's page with the session
  cookie; the shell checks the prefix, mints a request-bound token
  (`htm`/`htu`), calls the platform, and returns status and body unchanged.
  Writes need `Author`, carry an idempotency key, and are never retried.
- **Liveness:** the shell's one event subscription per platform
  ([[HLIN-A-0011]]) is relayed to mounted modules as `changed`, as are
  modules' own `changed` messages.

## Detailed Design

Slices, each demoable when it lands:

1. **The bridge, specified.** A new specification: message types, fields,
   protocol versioning, timeouts, heartbeat, and the states each failure maps
   to. Written first, because it is the new public API.
2. **Manifest.** `ui` on panels and navigation entries (entry document, bridge
   major), `assets` and `routes` prefixes (reads and writes separately) on the
   platform. Validation, canonical form, contract hash and diff. Amends
   [[HLIN-S-0001]].
3. **Bound tokens.** `hlin-identity` mints and verifies `htm`/`htu`, with a
   platform-side helper that refuses unbound or mismatched tokens on writes.
   Amends [[HLIN-S-0004]]. Shared with [[HLIN-I-0010]].
4. **Asset proxy.** `/m/{platform}/…`: fetch under the declared prefix only,
   content-hash caching, correct `application/wasm` types, the module CSP,
   bounded sizes.
5. **Request proxy.** `/p/{platform}/…`: prefix check, bound token, `Author`
   on writes, `Idempotency-Key` passthrough, bounded bodies, the shell's own
   refusals worded, platform answers passed through untouched.
6. **Frame host in `hlin-ui`.** A panel kind that mounts a sandboxed frame;
   the parent end of the bridge with source checking; `ready` timeout and
   heartbeat mapped onto panel states; fallback to shell-drawn data; mount on
   scroll into view; the drag shield over frames.
7. **Context and liveness.** `init`, `context`, `theme` and `visibility` sent
   down; `set-param`, `set-range`, `navigate` and `notice` acted on; `changed`
   relayed from platform events and from modules.
8. **The SDK.** A crate for Leptos modules: context and theme as signals,
   `fetch` as an async call, `ready` and heartbeat handled, idempotency keys on
   writes, theme applied to the frame's document.
9. **Pages.** A navigation entry opens a platform page at full width through
   the same host.
10. **Proof.** A small module added to the existing sample platform, and
    browser tests: it draws, it follows the time picker, it makes a request as
    the viewer, and it cannot escape (the containment cases in Goals). A load
    time measurement for a surface of several modules.

## Alternatives Considered

Settled at the decision level; see [[HLIN-A-0014]] (shell renders
everything, hypermedia, modules in the page, frames holding a bearer
capability) and [[HLIN-A-0013]] (per-action declarations, direct browser
calls, blind passthrough).

## Open Questions

Carried from [[HLIN-A-0014]], to settle in design:

- **Prefix granularity:** one read and one write prefix per platform, or per
  panel and page.
- **Streaming over the bridge:** whether `fetch` needs a streaming form, or
  `changed` plus refetch is enough.
- **Kit drift:** whether the shell should refuse or flag a module built
  against a kit version too far from its own.
- **Budgets:** a per-surface limit on mounted frames, and what the shell shows
  past it.
- **Platforms' own frontends:** whether they keep running beside Hlin or shrink
  into modules.

## Implementation Plan

Not decomposed. Expected order: 1 first; 2 and 3 in parallel; 4 and 5 once 2
and 3 land; 6 and 8 together, since each is the other's test; then 7, 9 and
10. [[HLIN-I-0010]] can start its platforms' server side and Dex once 3 lands.

## Status Updates

### 2026-09-24 — opened

Opened after [[HLIN-A-0013]] and [[HLIN-A-0014]] were decided and the vision
amended. Architecture is set by those decisions; design here is the bridge
specification and the open questions above.
