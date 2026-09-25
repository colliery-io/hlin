---
id: the-feed-platform-posts-and-who
level: task
title: "The feed platform: posts, and who may write them"
short_code: "HLIN-T-0073"
created_at: 2026-09-25T00:39:00.624391+00:00
updated_at: 2026-09-25T00:39:43.398538+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The feed platform: posts, and who may write them

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 4 of [[HLIN-I-0010]]: the server half of the feed. Its module is
[[HLIN-T-0075]].

## Acceptance Criteria

## Acceptance Criteria

- [ ] A new crate `hlin-sample-feed`, a binary with `--port` (default 8084)
      and `--bind`, holding state in memory, seeded with a few posts
- [ ] A manifest: `routes` under `/api/`, a panel `posts` drawn by the shell
      as a `table` (`records.v1`), newest first. No `ui` yet
- [ ] JSON API under `/api/`: list posts; create, edit and delete a post.
      Every request verified with `hlin-identity`; writes require a
      request-bound token
- [ ] Rules from claims (ABAC): anyone signed in reads; only `@example.com`
      addresses may post; authors edit and delete their own; 403 otherwise,
      in the platform's own words
- [ ] A platform-local setting: `--muted <email>` (repeatable) names people
      who may not post, a per-user rule that lives nowhere but the platform
- [ ] `Idempotency-Key` honoured on writes
- [ ] An event stream announcing every change; the panel declares `pushed`
- [ ] Tests: rules table-driven with a token per user (alice, bob, carol, a
      muted user); unbound and mismatched write tokens refused; idempotency;
      the manifest validates
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Same shape as [[HLIN-T-0072]]; independent code, so each can be copied on
  its own.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.
