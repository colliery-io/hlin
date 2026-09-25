---
id: bring-up-the-collaborative-demo
level: task
title: "Bring up the collaborative demo with both platforms side by side"
short_code: "HLIN-T-0076"
created_at: 2026-09-25T00:39:03.968325+00:00
updated_at: 2026-09-25T00:39:03.968325+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# Bring up the collaborative demo with both platforms side by side

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 7 of [[HLIN-I-0010]].

## Acceptance Criteria

- [ ] `angreal demo up --with collab` builds both modules, starts Postgres,
      Dex, the checklist, the feed and the shell; `demo down` stops them
- [ ] `demo/hlin-collab.toml` configures both platforms on `hlin-token`
- [ ] A published layout with the checklist and the feed side by side, which
      is what a person lands on after signing in
- [ ] README's collaborative demo section describes the story

## Implementation Notes

- Builds on [[HLIN-T-0061]]. Blocked on [[HLIN-T-0074]] and [[HLIN-T-0075]]
  for the modules; the platforms and layout can land first with the
  shell-drawn tables.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.
