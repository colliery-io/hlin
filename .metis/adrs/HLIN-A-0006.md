---
id: 001-the-shell-s-persistent-store-is
level: adr
title: "The shell's persistent store is Postgres"
number: 1
short_code: "HLIN-A-0006"
created_at: 2026-09-07T11:59:39.150029+00:00
updated_at: 2026-09-07T12:00:43.776520+00:00
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

# ADR-6: The shell's persistent store is Postgres

## Context

The design review established that the shell is stateful. Last-seen manifests must survive restarts or contract-violation detection has a gap across every restart window ([[HLIN-A-0002]]). Layouts are user-authored and must persist. Team sharing of layouts, an open question in the vision, will need ownership records. Per-principal option caches and staleness bookkeeping are runtime state that may or may not need durability.

The store's choice shapes deployment (single binary vs. external service), the shell's ability to run more than one instance, and the local development story.

## Decision

The shell's persistent store is Postgres from day one. Every durable shell fact lives there: last-seen manifests and contract hashes per platform, layouts and their ownership, and whatever sharing model the layout-persistence work decides. Access is behind a store trait in the shell so that tests run against an in-memory implementation, but Postgres is the only production backend and no other backend is planned.

Runtime caches (per-principal option lists, in-flight dedup tables, staleness timers) are process-local and are not persisted.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Postgres from day one (chosen) | Multi-instance shell and team sharing need nothing new later; one well-understood operational dependency; the platforms already run services with databases | External service in every deployment and in local dev | Low | Medium |
| SQLite embedded | Single-binary deploy; no external dependency | One instance at a time or a shared volume; migration to Postgres later is exactly the kind of rework the design initiative exists to avoid | Medium | Low |
| Trait only, defer the backend | No commitment now | Last-seen manifests do not survive restarts until a backend exists, so ADR-2 enforcement has a gap precisely while the first platforms are being onboarded | High | Low |

## Rationale

The two things the store exists for, contract memory and layouts, are both things the system is wrong without. A backend that has to be swapped later would put the swap in the path of the first multi-instance deployment or the first shared layout, which are the moments the shell is proving its value. Postgres removes that cliff at the cost of a dependency the surrounding platforms already carry.

## Consequences

### Positive
- The shell can run as multiple instances behind the routing layer without any coordination beyond the database.
- Layout sharing, ownership, and the "layout references a platform the viewer cannot access" question can be designed against real tables rather than a hypothetical backend.
- Contract-violation detection has no restart gap.

### Negative
- Local development needs a Postgres; the angreal task set must provide one (containerized) so `angreal test` and `angreal run` are self-sufficient.
- Schema migrations become part of the shell's release process from the first release.

### Neutral
- The store trait exists for testability, not for portability; an in-memory implementation is a test double, not a supported backend.
- Which crate owns the store is a crate-layout question: it lives in the shell binary unless a later need pulls it out.
