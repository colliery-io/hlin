---
id: the-checklist-platform-lists
level: task
title: "The checklist platform: lists, members and their rules"
short_code: "HLIN-T-0072"
created_at: 2026-09-25T00:38:59.633507+00:00
updated_at: 2026-09-25T00:39:42.665143+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The checklist platform: lists, members and their rules

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 3 of [[HLIN-I-0010]]: the server half of the checklist, a reference
a platform team can copy. Its module is [[HLIN-T-0074]].

## Acceptance Criteria

## Acceptance Criteria

- [ ] A new crate `hlin-sample-checklist`, a binary with `--port` (default
      8083) and `--bind` like `hlin-sample-platform`, holding state in memory
- [ ] Lists, each with an owner and members, kept by the platform itself.
      Seeded: `team` (owner alice@example.com; members alice and bob) and
      `carol` (owner and only member carol@elsewhere.org), with a few items
- [ ] A manifest at the well-known path: `routes.read` and `routes.write`
      under `/api/`, and a panel `items` drawn by the shell as a `table`
      (`records.v1`) with a `select` parameter `list` whose options are the
      lists the viewer belongs to. No `ui` yet: [[HLIN-T-0074]] adds it
- [ ] JSON API under `/api/`: read a list's items; add, toggle, edit and
      delete an item. Every request verified with `hlin-identity`; writes
      require a request-bound token ([[HLIN-T-0063]]'s extractor)
- [ ] Rules, decided by the platform from the token's `email`/`sub` and its
      own membership data: members read, add and toggle; the item's author or
      the list's owner edit and delete; anyone else gets 403 with a short
      message in the platform's own words
- [ ] Writes honour `Idempotency-Key`: a repeated key returns the first
      answer without applying the change twice (bounded memory of recent keys)
- [ ] An event stream in the manifest (`events`), announcing every change with
      the panel and its `list` selection, following `hlin-sample-platform`'s
      `changes.rs`; the panel declares `pushed`
- [ ] Tests: the rules, table-driven with a token per user; unbound and
      mismatched write tokens refused; idempotency; the manifest validates
      with `hlin-manifest`
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Copy the shape of `crates/hlin-sample-platform` (manifest.rs, routes.rs,
  changes.rs, main.rs) rather than depending on it: this is a reference to
  copy.
- Items carry `id`, `text`, `done`, `author` (display name) and `author_id`.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.
