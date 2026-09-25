---
id: find-why-a-warm-visit-downloads-a
level: task
title: "Find why a warm visit downloads a module again"
short_code: "HLIN-T-0088"
created_at: 2026-09-25T12:37:22.248049+00:00
updated_at: 2026-09-25T17:09:22.091208+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Find why a warm visit downloads a module again

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0084]]: on a warm visit (same browser context, reload),
the kanban widget's 0.6 MB wasm was downloaded again every time, while the
other modules came from cache. Cause unknown.

## Acceptance Criteria

## Acceptance Criteria

- [ ] The cause found and written down (cache headers from the platform, the
      asset proxy's `Cache-Control`/`ETag` handling, Trunk's hashed file
      names, or the module itself)
- [ ] Fixed where it belongs, with a test that a second visit fetches no
      module wasm that has not changed
- [ ] Warm bytes for "Twenty" re-measured and recorded in [[HLIN-I-0012]]

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.
