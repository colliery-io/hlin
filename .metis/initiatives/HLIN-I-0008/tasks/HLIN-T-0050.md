---
id: refuse-every-write-and-say-why
level: task
title: "Refuse every write, and say why"
short_code: "HLIN-T-0050"
created_at: 2026-09-09T00:30:33.019498+00:00
updated_at: 2026-09-09T00:30:33.019498+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# Refuse every write, and say why

## What

Under `anonymous`, every write is refused with 403 and a reason a person can
act on.

## The writes

- `POST /api/layouts` (create)
- `PUT /api/layouts/{id}` (replace)
- `DELETE /api/layouts/{id}` (remove)
- `POST /api/layouts/{id}/fork`

Not `POST /api/stream/{id}/params`: a time range and a filter selection live in
the in-memory surface, keyed per visitor. Driving a control is the product
working, not a write.

## Shape

Refuse in one place, not four. The natural home is beside the `Caller`
extractor — an extractor that yields a principal *and* the right to change
things, so a handler that mutates says so in its signature and cannot be
written without it. That is the pattern the codebase already uses for
authentication, and it makes a future mutating handler fail to compile rather
than fail open.

## Done when

- Each of the four answers 403 under `anonymous`, with a body naming the reason.
- Each of the four still works under `dev`, so the guard is about the strategy
  and not about the route.
- Reads and `set_params` are untouched.
- A test per route, and one asserting a new mutating handler cannot be written
  without the extractor.

## Status Updates

- 2026-09-09: Done. An `Author` extractor beside `Caller` in `identity.rs`:
  a mutating handler asks for it in its signature, so the guard cannot be
  forgotten. `create`, `replace`, `remove` and `fork` swapped to it. 403 with
  `{"error":"read_only","detail":...}`.
- Found while doing it: `home` was a write. It creates a layout when the
  caller has none, so an open shell would have written a row per browser that
  ever arrived — an unbounded write on a shell whose premise is that it stores
  nothing visitors do. Under read-only it returns the most recently published
  layout, and says plainly when nothing has been published rather than
  inventing an empty surface.
- `set_params` deliberately untouched: a time range and a filter selection live
  in the in-memory surface, keyed per visitor.
