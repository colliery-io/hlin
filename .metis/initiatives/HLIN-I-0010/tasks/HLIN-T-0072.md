---
id: the-checklist-platform-lists
level: task
title: "The checklist platform: lists, members and their rules"
short_code: "HLIN-T-0072"
created_at: 2026-09-25T00:38:59.633507+00:00
updated_at: 2026-09-25T00:53:59.694564+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


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

## Acceptance Criteria

- [x] A new crate `hlin-sample-checklist`, a binary with `--port` (default
      8083) and `--bind` like `hlin-sample-platform`, holding state in memory
- [x] Lists, each with an owner and members, kept by the platform itself.
      Seeded: `team` (owner alice@example.com; members alice and bob) and
      `carol` (owner and only member carol@elsewhere.org), with a few items
- [x] A manifest at the well-known path: `routes.read` and `routes.write`
      under `/api/`, and a panel `items` drawn by the shell as a `table`
      (`records.v1`) with a `select` parameter `list` whose options are the
      lists the viewer belongs to. No `ui` yet: [[HLIN-T-0074]] adds it
- [x] JSON API under `/api/`: read a list's items; add, toggle, edit and
      delete an item. Every request verified with `hlin-identity`; writes
      require a request-bound token ([[HLIN-T-0063]]'s extractor)
- [x] Rules, decided by the platform from the token's `email`/`sub` and its
      own membership data: members read, add and toggle; the item's author or
      the list's owner edit and delete; anyone else gets 403 with a short
      message in the platform's own words
- [x] Writes honour `Idempotency-Key`: a repeated key returns the first
      answer without applying the change twice (bounded memory of recent keys)
- [x] An event stream in the manifest (`events`), announcing every change with
      the panel and its `list` selection, following `hlin-sample-platform`'s
      `changes.rs`; the panel declares `pushed`
- [x] Tests: the rules, table-driven with a token per user; unbound and
      mismatched write tokens refused; idempotency; the manifest validates
      with `hlin-manifest`
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Copy the shape of `crates/hlin-sample-platform` (manifest.rs, routes.rs,
  changes.rs, main.rs) rather than depending on it: this is a reference to
  copy.
- Items carry `id`, `text`, `done`, `author` (display name) and `author_id`.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 (implemented)

New crate `crates/hlin-sample-checklist` (binary and library), copying the
sample platform's shape without depending on it. Every criterion met;
`angreal check all` and `angreal test all` pass.

- `lists.rs`: the data and the rules, with nothing about HTTP in it. Membership
  by lowercased `email`, authorship by `sub`, display by `name`. Refusals are a
  `Refused` enum carrying status and a sentence.
- `routes.rs`: reads take `HlinIdentity`, writes `HlinRequestIdentity`.
  Checking the key, doing the write and remembering its answer happen under
  one lock, so two concurrent retries cannot both apply; the change is
  announced after the lock is released.
- `idempotency.rs`: the last 1024 keys, scoped per caller (`sub`), keyed
  against method, path and body.
- `changes.rs`: `changed` events as `{"panel":"items","selections":{"list":[id]}}`;
  a lagging subscriber gets `{"panel":"items"}`.
- Tests in `tests/integration.rs` mint real read and bound tokens per user
  with an `Issuer` and drive the router in process: 27 table-driven rule
  cases, unbound, mismatched (path, method, list, audience) and absent write
  tokens, idempotency, the event stream, the panel envelopes and the manifest.

API for [[HLIN-T-0074]]:

| Method | Path | Answer |
|---|---|---|
| GET | `/api/lists` | `{"lists":[{id,name,owner,members}]}`, the viewer's only |
| GET | `/api/lists/{list}/items` | `{"list":{...},"viewer":{"id","owner"},"items":[{id,text,done,author,author_id}]}` |
| POST | `/api/lists/{list}/items` | body `{"text"}`; 201 with the item |
| POST | `/api/lists/{list}/items/{item}/toggle` | 200 with the item |
| PATCH | `/api/lists/{list}/items/{item}` | body `{"text"}`; 200 with the item |
| DELETE | `/api/lists/{list}/items/{item}` | 204 |
| GET | `/api/hlin/items?list=` | `records.v1`, the panel's data |
| GET | `/api/hlin/lists` | `options.v1`, the picker's options |
| GET | `/api/events` | SSE |

Refusals: 403 not a member / not author or owner / no email, 404 no such
list or item, 400 bad text or body, 422 a key reused for a different write,
401 (from `hlin-identity`) for a missing, unbound or mismatched token.

Decisions the task left open:

- A list that does not exist is 404 and one that exists but is not yours is
  403, which says a list exists. Chosen because the initiative wants the
  refusal worded ("anyone else reading a list gets 403").
- Toggle flips rather than sets; a retry is covered by the key. Items cross
  off by any member, edit and delete by author or owner.
- Seeded items are authored by each list's owner, with the owner's email as
  `author_id`, since no `sub` exists before anyone signs in; the owner rule is
  what lets them change.
- `Idempotency-Key` is optional (curl still works); a malformed one is 400.
  Refusals are remembered and replayed like successes. A replay carries
  `Idempotent-Replayed: true`.
- The list response tells the platform's own module `viewer.id` and
  `viewer.owner`, so it can hide what would be refused. That goes to the
  module, not the shell, which relays it opaquely (decision 5).
- No `--auth open`: an anonymous caller is on no list. Driving it without a
  shell is the opt-in `dev-identity` feature (development principal
  `alice@example.com`), which `hlin-identity` refuses in release builds.
- Not added to `[workspace.dependencies]`: nothing depends on it.
