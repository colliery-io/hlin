---
id: open-a-platform-s-page-inside-the
level: task
title: "Open a platform's page inside the shell"
short_code: "HLIN-T-0070"
created_at: 2026-09-25T00:01:09.187717+00:00
updated_at: 2026-09-25T00:01:09.187717+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Open a platform's page inside the shell

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 10 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Pages*: a navigation entry that
declares `ui` opens at full width in the same kind of frame, with
`init.page` set.

## Acceptance Criteria

- [ ] Navigation entries with `ui` appear in the shell's navigation and open a
      full-width frame on a route of the shell's own (so a page has a URL a
      person can share)
- [ ] Same frame rules, bridge, states and fallback-free behaviour as panels
      (a page has no shell-drawn fallback; it is `unavailable` and says so)
- [ ] `navigate` from any module can open a page
- [ ] Entries without `ui` keep behaving as today
- [ ] Browser test: open a page from navigation and from `navigate`
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0066]] and [[HLIN-T-0067]].
- `crates/hlin-ui/src/app.rs` already reads navigation.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
