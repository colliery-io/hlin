---
id: a-module-whose-platform-is-down
level: task
title: "A module whose platform is down says so, and recovers when it returns"
short_code: "HLIN-T-0087"
created_at: 2026-09-25T12:37:21.307620+00:00
updated_at: 2026-09-25T12:37:21.307620+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# A module whose platform is down says so, and recovers when it returns

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Found by [[HLIN-T-0084]]. Killing one widget's platform (dice) affects no
other panel, which is right. But on a page already open, the dice panel stays
`ready`: the module is alive, so by [[HLIN-S-0007]] the shell has no reason to
change its state, and the module shows "can't reach its platform". It never
goes `stale`, and when the platform returns nothing recovers until a person
clicks "Try the module again" (0.85 s) or reloads. A widget whose data fetch
failed offers no way to try again at all.

Two questions, one for the owner:
- Should repeated `unreachable` answers from `/p/` for a platform count
  towards that panel's state (a module can be alive while its platform is
  dead)? Proposed: yes — the page already sees every `/p/` answer; after N
  consecutive `unreachable`/`timeout` refusals for one platform, its panels go
  `stale`, and the first success clears it. The shell reads status codes it
  produced itself, not what the platform says, so HLIN-A-0013's "the platform
  decides" is untouched.
- When a platform comes back: the shell already follows its event stream;
  a reconnected stream should count as a `changed` for all its panels, so
  modules refetch without anyone clicking.

## Acceptance Criteria

- [ ] Owner's answer to the first question recorded, and implemented
- [ ] A platform that returns is noticed and its modules refetch without a
      reload or a click
- [ ] Widgets (via `hlin-widget-module`) show a way to try again when a load
      fails, and retry on their own on `changed`
- [ ] The twenty measurement's kill-and-restart run shows the panel degrade
      and recover by itself; recorded in [[HLIN-I-0012]]'s results
- [ ] `angreal check all`, `angreal test all`, browser suites pass

## Status Updates

### 2026-09-25

Created from [[HLIN-T-0084]]'s findings. Not started.

### 2026-09-25 — owner's answer

Yes, from the shell's own refusals. After a few consecutive `unreachable` or
`timeout` answers from `/p/` for one platform (codes the shell itself
produced), that platform's panels go `stale`; the first success clears it.
When the platform's event stream reconnects, its modules are told to refetch,
so recovery needs no click.
