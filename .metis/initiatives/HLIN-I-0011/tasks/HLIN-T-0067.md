---
id: give-modules-the-surface-s-context
level: task
title: "Give modules the surface's context, relay what changed, and keep a surface within its frame budget"
short_code: "HLIN-T-0067"
created_at: 2026-09-25T00:01:05.391426+00:00
updated_at: 2026-09-25T00:01:05.391426+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


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

- [ ] `context` sent on every time-range or parameter change, carrying only
      declared parameters; `theme` on scheme or token change; `visibility`
      on scroll and tab visibility
- [ ] `set-param` and `set-range` have exactly the effect of the chrome's
      controls and `Intent::Select`/`Intent::Range`, stored with the layout
- [ ] `changed` from a platform's event stream reaches that platform's mounted
      modules; a module's `changed` is relayed with `from: "module"` to that
      platform's other modules on every surface this shell serves (needs a
      shell-side relay, not only the page)
- [ ] `navigate` opens a panel or page; unknown targets ignored and logged
- [ ] `notice`: plain text, at most 140 characters, labelled as the
      platform's, in that panel's frame only
- [ ] Budget: at most 12 mounted; the least recently seen out-of-view frame is
      unmounted first; never one in view; `suspend` then `state` (at most
      `state_bytes`, 500 ms deadline) kept in page memory and returned as
      `init.restored`
- [ ] Tests for each, browser tests for budget and relay across two browser
      contexts
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0066]].
- The shell already follows each platform's event stream
  (`crates/hlin/src/stream/streams.rs`); the relay to modules needs a path
  from there, and from `/p/`-carried module `changed` messages, to the browser.
  The existing surface stream is the obvious carrier.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
