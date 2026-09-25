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

- [x] A new crate `hlin-sample-feed`, a binary with `--port` (default 8084)
      and `--bind`, holding state in memory, seeded with a few posts
- [x] A manifest: `routes` under `/api/`, a panel `posts` drawn by the shell
      as a `table` (`records.v1`), newest first. No `ui` yet
- [x] JSON API under `/api/`: list posts; create, edit and delete a post.
      Every request verified with `hlin-identity`; writes require a
      request-bound token
- [x] Rules from claims (ABAC): anyone signed in reads; only `@example.com`
      addresses may post; authors edit and delete their own; 403 otherwise,
      in the platform's own words
- [x] A platform-local setting: `--muted <email>` (repeatable) names people
      who may not post, a per-user rule that lives nowhere but the platform
- [x] `Idempotency-Key` honoured on writes
- [x] An event stream announcing every change; the panel declares `pushed`
- [x] Tests: rules table-driven with a token per user (alice, bob, carol, a
      muted user); unbound and mismatched write tokens refused; idempotency;
      the manifest validates
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Same shape as [[HLIN-T-0072]]; independent code, so each can be copied on
  its own.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 (implemented)

New crate `crates/hlin-sample-feed` (binary `hlin-sample-feed`, `--port`
8084, `--bind`, `--name feed`, `--domain example.com`, `--muted` repeatable,
`--shell-keys`, `--shell-issuer`). Every criterion met; `angreal check all`
and `angreal test all` pass.

API, all under `/api/`, every route but health behind
`HlinRequestIdentity` (so writes need a token bound to their method and
path; a refusal there is the extractor's 401 with `reason`):

- `GET /api/posts` → `{ "posts": [Post] }`, newest first
- `POST /api/posts` `{ "body" }` → 201 `Post`
- `PUT /api/posts/{id}` `{ "body" }` → 200 `Post`
- `DELETE /api/posts/{id}` → 204
- `GET /api/panels/posts` → `records.v1` (the `posts` panel's `data`)
- `GET /api/events` → SSE `changed` `{ "panel": "posts" }`, 15 s heartbeat
- `Post` = `{ id, author: { id (= sub), name }, body, posted_at, edited_at }`

Refusals the feed decides are `{ "message" }`: 400 (no or bad
`Idempotency-Key`, empty or over-500-character body, not JSON), 403 ("Only
people at example.com can post here", "You have been muted on this feed",
"Only the author can edit/delete this post"), 404 ("There is no such post"),
422 (key reused for a different request).

Decisions the task left open:

- Muted people may not create *or edit* (rewriting is posting), but may
  delete their own posts.
- The domain match is the whole domain after the last `@`,
  case-insensitive; no `email` claim means no posting. Muted emails compare
  case-insensitively.
- `Idempotency-Key` is required on writes (400 without), scoped per `sub`,
  bound to method, path and body bytes (422 on mismatch), and only writes
  that happened are remembered (the last 1024), so a refused write is
  decided afresh on retry. A replay carries `Idempotency-Replayed: true`.
- Rules are checked before the draft, so someone who may not edit a post
  learns that rather than that their words were too long.
- Seeded posts are by `seed-dana` and `seed-eli`, whom nobody signs in as,
  so nobody in the demo can edit them. No `/api/me`: nothing about the rules
  is described in advance (decision 5); a module compares `author.id` with
  the viewer's id if it has one, or just shows the refusal.
- A `dev-identity` cargo feature forwards to `hlin-identity`'s bypass, with
  `developer@{domain}` as the development principal.
