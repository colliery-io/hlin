---
id: 001-layouts-are-personal-or-org
level: adr
title: "Layouts are personal or org-published; sharing is read-only with fork to edit"
number: 1
short_code: "HLIN-A-0007"
created_at: 2026-09-07T12:12:55.920873+00:00
updated_at: 2026-09-07T12:14:07.535543+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-7: Layouts are personal or org-published; sharing is read-only with fork to edit

## Context

The vision leaves layout persistence and sharing open: personal, team, or both, and what ownership means for a layout that references a platform the viewer cannot access. Two prior decisions give the question ground to stand on. The shell has a Postgres store ([[HLIN-A-0006]]), so ownership records are real tables. Identity is forwarded and platforms authorize per request ([[HLIN-A-0004]]), so the shell already knows who is looking and never needs to know what they are allowed to see.

The success condition is the constraint: a panel shipped Tuesday morning is on someone else's dashboard Tuesday afternoon with no conversation between teams. Sharing has to be frictionless; governance has to be minimal.

## Decision

**Ownership.** A layout has exactly one owner, the principal who created it. There are no team entities in v1. A layout is in one of two visibility states: *personal* (visible only to its owner and to anyone holding its link) or *published* (listed in a shell-wide gallery visible to every authenticated principal). The owner toggles between them; publishing is one action and needs no approval.

**Sharing.** Anyone who opens a layout they do not own gets a read-only view of the owner's live layout and receives the owner's later edits. Editing forks: the viewer gets their own copy, owned by them, with a recorded `forked_from`. There is one source of truth per layout and no concurrent editing.

**Referenced platforms the viewer cannot access.** Nothing special. The layout renders; each panel's data fetch carries the viewer's identity; a panel the platform refuses renders as `unavailable (forbidden)` per the state machine ([[HLIN-S-0002]]). The layout engine never inspects authorization and never hides a panel. A layout is a list of references, not a grant.

**Layout content.** A layout stores, per panel instance: the reference `platform.id/key`, the chosen kind if it differs from the manifest default, the title override, `select` parameter selections, composition-level customizations such as thresholds, and position. The global time range is layout state too, so a shared layout opens at the range its author meant.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Personal + org-published, read-only share with fork (chosen) | No team model; publishing is one action; one source of truth per layout; owners' improvements reach everyone subscribed | The gallery can fill with noise; no way to hand a layout to a group as a group | Low | Low |
| Personal + team ownership | Group-editable layouts | Needs a team concept, membership from token claims, and per-team authorization rules in v1; all of it is guessing at org structure | Medium | Medium |
| Personal only, link sharing | Smallest possible | No discovery: the Tuesday-afternoon person has to know the link exists | Low | Low |
| Fork on open | No subscription mechanics | Owner's later improvements never reach anyone who already opened the layout; the gallery becomes a graveyard of stale copies | Medium | Low |
| Live co-editing | Group authorship | Conflict handling and ACLs in v1 | High | High |

## Rationale

Teams are the org chart, and the org chart is exactly the coordination structure the vision is designed to route around. A published gallery gives discovery without asking the shell to know who works with whom. Read-only-with-fork gives every layout one author, which keeps ownership legible and makes "the owner improved it" propagate for free, while forking keeps anyone from being blocked on the owner.

Leaving forbidden panels visible is deliberate. Hiding them would require the layout engine to know platform policy, which [[HLIN-A-0004]] forbids, and would make a shared layout silently differ between viewers. A visible forbidden panel tells the viewer something true: this exists, and you cannot see it.

## Consequences

### Positive
- The last open design question in the vision is closed; the initiative can decompose.
- The store schema for v1 is small: layouts, panel instances, an owner, a visibility flag, a `forked_from` pointer.
- The success condition holds with zero governance: publish, open, fork.

### Negative
- Gallery quality is unmanaged. The mitigation is social and cheap: sort by recent use, show the owner, let owners unpublish. A curation mechanism, if ever needed, is a later ADR.
- No group ownership means a layout dies with its owner's departure unless someone forked it. Ownership transfer is a small feature to add when it first hurts.

### Neutral
- A layout referencing a removed platform or panel renders those instances as `unavailable (unknown)` with the successor hint if the manifest ever declared one; layouts are never automatically rewritten.
- Team ownership can be added later as a second owner type without changing the sharing model.
