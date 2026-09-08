---
id: deduplicate-per-principal-or-amend
level: task
title: "Deduplicate per principal, or amend HLIN-A-0004"
short_code: "HLIN-T-0032"
created_at: 2026-09-08T01:56:14.104130+00:00
updated_at: 2026-09-08T07:07:36.836511+00:00
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

# Deduplicate per principal, or amend HLIN-A-0004

## Objective

HLIN-A-0004 says: "Identical requests from the same principal collapse across
every panel and every open surface that principal has." The code collapses
across panels on one surface, and two tabs on the same layout share a surface.
Two *different* layouts showing the same panel with the same parameters produce
two upstream requests.

Finding 7 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (ADR drift)

### Priority
- [x] P2 - Medium

### Technical Debt Impact
- **Current Problems**: `aggregator.rs:395` `wanted` is local to one
  `Surface::due()`; `surfaces.rs:48` keys by `{layout}:{principal}`; nothing is
  shared in `Surfaces`. The per-principal boundary — the security property — is
  correctly enforced. The efficiency claim is narrower than written.
- **Benefits of Fixing**: Either the ADR says what the code does, or a principal
  with several surfaces open stops paying twice.
- **Risk Assessment**: Low. This is honesty rather than exposure.

## Acceptance Criteria

Either:

- [x] HLIN-A-0004 amended to "across every panel on a surface; a layout opened
      twice by one principal is one surface"

Or:

- [ ] A per-principal in-flight table — **not taken**; see the status update in `Surfaces`, keyed on `Request::key()`
      plus principal, with a test that two surfaces asking the same question
      produce one upstream request

## Implementation Notes

### Technical Approach
Amending is the cheap, honest option and should be the default unless a
principal routinely holds several surfaces.

### Dependencies
None.

## Status Updates

### 2026-09-08 — amended, not implemented

HLIN-A-0004 now carries a dated refinement saying what the code does: collapsing
is across every panel *on a surface*, which covers two panels reading the same
endpoint and covers a layout opened in two tabs, since one principal opening one
layout gets one surface. Two different layouts showing the same panel produce two
requests.

**The security half is untouched and is the half that matters**: the principal is
part of the key, a surface exists per `(layout, principal)`, and nothing is ever
shared across principals. Only the efficiency claim was wider than the code.

Amending was chosen over implementing on the reasoning in the refinement itself:
a per-principal in-flight table is shared mutable state across surfaces, with its
own lifetime and eviction, bought for a case that only arises when one person
routinely keeps several *different* layouts open at once. The ADR now names that
as the condition for revisiting it, so the decision to defer is recorded where
somebody would look rather than only here.
