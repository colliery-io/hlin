---
id: prove-and-measure-the
level: task
title: "Prove and measure the containerised twenty"
short_code: "HLIN-T-0095"
created_at: 2026-09-25T23:55:00.609203+00:00
updated_at: 2026-09-25T23:55:00.609203+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0013
---

# Prove and measure the containerised twenty

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Run the twenty browser suite and the twenty measurement against the
containers, signed in through Dex, and record the numbers beside the
process-based ones in [[HLIN-I-0012]].

## Acceptance Criteria

- [ ] `angreal e2e twenty` and `twenty-measure` (or container-aware variants)
      pass against the compose deployment, signed in through Dex
- [ ] The kill-and-recover check uses `docker compose stop`/`start` on a
      widget container, twice in one session
- [ ] Each widget's core UI is reachable on the compose network at `/`, and
      an unknown `/hlin/*` path answers 404 (checked from inside the
      network)
- [ ] Numbers recorded (cold and warm first content, bytes, memory, peak
      frames), medians of 3, compared with [[HLIN-I-0012]]'s, with any
      difference explained
- [ ] README: how to run the containerised demo, and that it is the reference
      for how a platform should serve Hlin

## Implementation Notes

- Depends on [[HLIN-T-0094]].

## Status Updates

### 2026-09-25

Created. Not started.
