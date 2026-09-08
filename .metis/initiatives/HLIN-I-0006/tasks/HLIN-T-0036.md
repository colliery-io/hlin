---
id: let-a-platform-declare-how-fresh
level: task
title: "Let a platform declare how fresh its data is"
short_code: "HLIN-T-0036"
created_at: 2026-09-08T01:56:19.347634+00:00
updated_at: 2026-09-08T09:54:21.020899+00:00
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

# Let a platform declare how fresh its data is

## Objective

The refresh interval is one number per shell (`Timings::policy()`), applied to a
queue depth that changes eight times a second and a nightly batch count alike.
The publisher knows the answer and `hlin-manifest::Panel` has no field for it.

Finding 11 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (needs an ADR first)

### Priority
- [x] P2 - Medium

### Technical Debt Impact
- **Current Problems**: An operator sets the cadence for every panel from every
  platform. The wrong setting either misses live data or polls static data eight
  times a second.
- **Benefits of Fixing**: Cadence travels with the panel that knows it.
- **Risk Assessment**: This is a manifest change. Whether cadence is contract —
  probably not, like `kind`, it is a hint — needs deciding before the field
  exists, because envelope and contract vocabularies only grow.

## Acceptance Criteria

## Acceptance Criteria

- [x] An ADR deciding the shape: optional `refresh_ms` hint on `Panel`,
      non-contract, excluded from the contract hash, the shell's setting as a
      ceiling, staleness proportional to it
- [x] The aggregator honours it per instance
- [x] The sample platform's `live-*` panels declare it, and the default demo
      config no longer needs `hlin-live.toml` to show them moving

## Implementation Notes

### Technical Approach
`canonical.rs` already excludes `kind` for exactly this reason; the same
argument and the same exclusion apply.

### Dependencies
The ADR comes first. Related: [[HLIN-T-0033]] (staleness proportional).

## Status Updates

*Filed from the review; not started.*
