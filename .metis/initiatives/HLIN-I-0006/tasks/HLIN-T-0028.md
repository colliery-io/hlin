---
id: forward-only-the-parameters-a
level: task
title: "Forward only the parameters a panel declares"
short_code: "HLIN-T-0028"
created_at: 2026-09-08T01:56:08.854959+00:00
updated_at: 2026-09-08T02:58:13.013504+00:00
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

# Forward only the parameters a panel declares

## Objective

Selections posted by a browser become query parameters on the shell's
authenticated request to a platform, with no check against what the panel's
manifest declares. Any viewer can append `?anything=value` to an upstream call,
or override `from`, `to` and `step`.

Finding 3 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (security)

### Priority
- [x] P0 - Critical

### Technical Debt Impact
- **Current Problems**: Confirmed with a throwaway test against
  `Surface::set_params` then `due()`:

  ```
  selections = { "a": { "injected_by_viewer": ["true"] } }
  UPSTREAM QUERY: injected_by_viewer=true
  ```

  `server.rs:228` stores `request.selections` verbatim; `aggregator.rs:596`
  appends every `(key, value)`. The manifest's `ParamDecl`s are validated at
  ingestion (`validate.rs:353`) and never consulted again. No bound on key count
  or value length either: a large map produces a request line the platform
  refuses, which the shell reports as `malformed`, blaming the platform.
- **Benefits of Fixing**: The shell sends a platform exactly the parameters the
  platform said it responds to.
- **Risk Assessment**: A platform trusting `X-Hlin-Identity` and reading its
  query string reads it from whoever holds a browser. Today that runs as the dev
  principal ([[HLIN-T-0026]]); with real principals it runs as theirs, which is
  still more than the manifest promised them.

## Acceptance Criteria

- [x] In `Surfaces::instances_for` and `LiveSurface::set_params`, only keys a
      declared control names survive; the rest are dropped with a `warn!` naming
      the instance
- [x] `from`, `to`, `step` are reserved and never taken from selections
- [x] Keys per instance and bytes per value are capped
- [x] Test: a selection under an undeclared key never reaches `Request.query`
- [x] Test: a selection named `from` never reaches `Request.query`

## Implementation Notes

### Technical Approach
`instances_for` already has the manifest's declarations in hand when it parses
stored selections. Build the allowed set there, carry it on `Instance`, and have
`set_params` filter against it. `query_for` then needs no change.

### Dependencies
None. Independently useful before [[HLIN-T-0026]].

## Status Updates

### 2026-09-08 — the shell now believes the manifest it validates

`Instance` gained `accepts: BTreeSet<String>`, populated in
`Surfaces::instances_for` (and on the `all` path) from the panel's own
`ParamDecl`s — the same `config["id"]` the catalog reads to draw a control. A
declaration with no `id` grants nothing, which is right: `time_range` is driven
by the surface's one picker and names no per-panel key.

**Enforced in `query_for` rather than at the two write paths the ticket
suggested.** That is the single place a selection becomes a query parameter, so
it cannot be bypassed by a route added later; filtering only at `set_params`
would leave stored selections unchecked, and filtering only at
`instances_for` would leave live ones. One choke point, one rule.

Three guards, in order:

- `RESERVED` — `from`, `to`, `step` are never taken from a selection, even if a
  platform declares a control with that id. A platform should not be able to
  hand a viewer the ability to move the window behind the picker's back.
- `accepts` — only keys the panel declared.
- `MAX_VALUES` (32) and `MAX_VALUE_BYTES` (256) — an unbounded selections map
  becomes a request line a platform refuses, which the shell then reports as
  `malformed`, blaming a platform that did nothing wrong.

**Verified against the running demo**, repeating the probe from the review:

```
POST /api/stream/{id}/params  { "injected_by_viewer": ["true"],
                                "cluster": ["orebank-eu-west"] }   -> 202

occurrences of `injected_by_viewer` in orebank.log:  0
occurrences in shell.log (the drop being logged):   18
```

And the feature it guards still works: all four `interaction.spec.js` tests
pass, including the round trip that asserts the drawn series changes when a
declared filter is set.

**Three existing tests had to change**, and the change is the point. They built
instances with no declarations and then expected a selection to be forwarded —
correct before there was a rule, and describing a panel that cannot exist now.
Their fixtures declare the parameter they use, via a new `accepting()` helper.

287 Rust tests pass, 0 failures. `angreal check all` clean.
