---
id: fetch-a-surface-s-options-in-one
level: task
title: "Fetch a surface's options in one request"
short_code: "HLIN-T-0039"
created_at: 2026-09-08T01:56:23.104917+00:00
updated_at: 2026-09-08T04:37:31.864052+00:00
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

# Fetch a surface's options in one request

## Objective

`hlin-ui/src/app.rs:146` fetches `/api/options/{platform}/{panel}/{control}`
sequentially, once per declared control, on every page load. Fine for two
platforms with one listed control each; linear in controls, and a full round trip
for each.

Finding 14 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P3 - Low

### Technical Debt Impact
- **Current Problems**: Page load cost grows with the number of declared
  controls across every platform, whether or not a surface uses them.
- **Benefits of Fixing**: One round trip, and options that arrive before a
  person reaches for a filter.
- **Risk Assessment**: Low. Every control already falls back to a typed value,
  so a slow fetch costs convenience only.

## Acceptance Criteria

- [x] Either `/api/panels` includes choices with a short shell-side cache, or the
      browser fans out concurrently
- [x] The route keeps its no-caller-supplied-URL shape and its refusal test
- [x] Only controls on panels the surface actually holds are fetched

## Implementation Notes

### Technical Approach
The shell-side cache is the better shape: options are per platform and change
rarely, and the shell already holds the manifest that declares them.

### Dependencies
Shares the buffering fix in [[HLIN-T-0029]].

## Status Updates

### 2026-09-08 — only what the surface holds, fetched together

Measured on the running demo, counting `/api/options/` requests a page makes:

```
a surface of one panel with no controls   before: 2   after: 0
a surface of two panels that have them    after: 2, both concurrent
                                          orebank/throughput-faceted/cluster
                                          stampmill/throughput-by-cluster/cluster
```

**The shell-side cache was the other option and was not taken.** The shell
fetches options with the *viewer's* credential, and a platform may legitimately
answer two principals differently (HLIN-A-0004) — so a shared cache is a leak,
and a correct one has to be keyed per principal. That is more machinery than a
value this cheap to fetch and this rarely looked at deserves. The route keeps
its shape and its refusal test.

### The regression the first version introduced

Filtering to "panels on the surface" inside the catalogue fetch was wrong, and
compiling cleanly hid it: a panel added *after* the page loaded would never get
its options, because the fetch had already run. That is the ordinary composing
flow, not an edge case.

So it is its own effect, keyed on a `Memo` of the panel references. The memo
matters: the draft changes on every pointer move of a drag, and an effect
reading it directly would re-run through an entire gesture — the same trap that
once cost the stream effect a connection per frame. A memo of the sorted,
deduplicated references only notifies when the *set of panels* changes.

It also asks only for what it does not already have, so adding a second panel
from a platform does not re-fetch the choices the first one already got.

299 Rust tests, 23 browser tests, 1 honestly skipped. `angreal check all` clean.
