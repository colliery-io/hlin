---
id: a-manifest-may-declare-an-event
level: task
title: "A manifest may declare an event stream and which panels it reports on"
short_code: "HLIN-T-0042"
created_at: 2026-09-08T11:03:18.988212+00:00
updated_at: 2026-09-08T11:08:13.890030+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# A manifest may declare an event stream and which panels it reports on

## Parent Initiative

[[HLIN-I-0007]]

## Objective

Add `events` (platform-level) and `pushed` (per panel) to the manifest, per
[[HLIN-S-0006]]. Neither is contract: both are excluded from the contract hash,
so a platform adding or withdrawing an event stream is not a breaking change.

The smallest piece and the one everything else waits on — there is nothing to
subscribe to until a manifest can say where.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] `Manifest.events: Option<String>` and `Panel.pushed: bool` parse, round-trip,
      and default correctly for a manifest that declares neither
- [ ] Both are excluded from `contract_content`, proven by a test that changes
      each and asserts the hash is unmoved
- [ ] `events` is validated as a relative path by the same rule as `data` and
      `health` (REQ-1.2 of HLIN-S-0001): no absolute URLs, no `..`
- [ ] A panel marked `pushed` on a platform declaring no `events` is not an
      error — it is a platform that has not finished, and the shell simply polls
- [ ] The catalogue (`/api/panels`) carries `pushed`, because the browser's own
      staleness display will eventually want to know

## Implementation Notes

### Technical Approach

`crates/hlin-manifest/src/manifest.rs` for the fields, `validate.rs` for the
path rule, `canonical.rs` for the exclusion — which needs no change, since
`panel_contract` names its fields explicitly rather than excluding by list. That
is worth a test anyway: the exclusion currently holds by construction and should
fail loudly if anybody rewrites it as a denylist.

### Dependencies

None. Blocks every other task in this initiative.

### Risk Considerations

Low. The parse side is additive and `extra` already preserves unknown fields,
so an old shell reading a new manifest is unaffected — which is the property
REQ-1.1 depends on.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

`Manifest.events` and `Panel.pushed`, both optional, both absent by default, and
both proven not to move the contract hash by tests that add each to the
specification's own example and assert the fingerprint is unchanged. That
property is what makes push safe to adopt: a platform that adds a stream owes
nobody a major version.

**One thing changed from what this ticket said.** It asked for `events` to be
validated as a relative path "by the same rule as `data` and `health`", which is
right — but the same rule does not mean the same *consequence*. A bad `health`
path rejects the document, because the shell cannot tell whether the platform is
alive. A bad `events` path must not, because the shell was already correct
without it: rejecting would mean that getting an optional field wrong takes an
entire platform offline, which would make this feature more dangerous to adopt
than to skip.

So `Validation` gained `unusable_events: Option<PathDefect>` — informational,
alongside `newer_schema_version`, which is the precedent for "something worth
saying that changes nothing". The test covers an absolute URL, a scheme-relative
one and a `..` escape, and asserts in each case that every panel remains exactly
as offerable as before.

`pushed` reaches the catalogue for the reason `refresh_ms` does: how live a panel
is cannot be read off the shell's settings, and this is now half the answer.

324 Rust tests, up from 319.
