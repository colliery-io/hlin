---
id: let-a-pack-say-which-components-it
level: task
title: "Let a pack say which components it offers"
short_code: "HLIN-T-0037"
created_at: 2026-09-08T01:56:20.847804+00:00
updated_at: 2026-09-08T04:19:02.407728+00:00
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

# Let a pack say which components it offers

## Objective

A platform declaring `component: "aurora.grpah"` gets its declared kind,
silently — exactly as a shell running a pack without that component would.
`DesignPack::custom()` returns `None` for both, so a typo is indistinguishable
from an unsupported component, forever.

Finding 12 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P3 - Low

### Technical Debt Impact
- **Current Problems**: The fallback is correct and the silence hides mistakes.
- **Benefits of Fixing**: The catalog can say "this pack does not offer
  `aurora.grpah`" once, at ingestion, where a platform team would see it.
- **Risk Assessment**: Low. A typo costs a nicer rendering, never a broken panel.

## Acceptance Criteria

- [x] `DesignPack::offers(&self) -> &'static [&'static str]`, required like every
      other method; the demo pack returns `&[]` and means it
- [x] The frontend logs, once per panel, a component the mounted pack does not
      list
- [x] Rendering behaviour is unchanged: `custom()` still gets first refusal, and
      the declared kind still draws on `None`

## Implementation Notes

### Technical Approach
Keep it advisory. `offers()` must not gate `custom()`, or a pack could list a
component it declines and produce a hole.

### Dependencies
None.

## Status Updates

### 2026-09-08 — a typo can be told apart from an unsupported component

`DesignPack::offers() -> &'static [&'static str]`, required like every other
method. The demo pack returns `&[]` and means it; Aurora lists its three;
`Drawer` carries the list past type erasure the same way it already carries the
stylesheet, because the pack is gone by the time anything asks and whatever is
worth knowing has to be taken while it is still a concrete type.

The frontend logs once per panel when the mounted pack does not list the
component a platform asked for. That is the whole product change: the panel
already drew, and still does.

**Advisory, and the tests are about that.** `offers` must never gate `custom`,
because a pack that listed a component it then declined would produce a panel
nothing draws — precisely the hole the vocabulary exists to prevent. So there
are two tests rather than one:

- listed and offered draws via `custom`; not listed falls back to the declared
  kind, exactly as before `offers` existed
- a deliberately lying pack — one that lists `liar.component` and declines
  everything — still draws the declared kind. That is the case `offers` could
  have broken if anything had been allowed to gate on it, and it is why `custom`
  remains the authority on what actually draws.

298 Rust tests pass, 0 failures. The gallery front end builds. `angreal check
all` clean.
