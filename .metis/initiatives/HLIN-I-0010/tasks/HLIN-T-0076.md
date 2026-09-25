---
id: bring-up-the-collaborative-demo
level: task
title: "Bring up the collaborative demo with both platforms side by side"
short_code: "HLIN-T-0076"
created_at: 2026-09-25T00:39:03.968325+00:00
updated_at: 2026-09-25T00:54:23.845883+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# Bring up the collaborative demo with both platforms side by side

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 7 of [[HLIN-I-0010]].

## Acceptance Criteria

## Acceptance Criteria

- [ ] `angreal demo up --with collab` builds both modules, starts Postgres,
      Dex, the checklist, the feed and the shell; `demo down` stops them
- [x] `demo/hlin-collab.toml` configures both platforms on `hlin-token`
- [ ] A published layout with the checklist and the feed side by side, which
      is what a person lands on after signing in (met for Alice, who publishes
      it; not for anyone else, see the question below)
- [x] README's collaborative demo section describes the story

## Implementation Notes

- Builds on [[HLIN-T-0061]]. Blocked on [[HLIN-T-0074]] and [[HLIN-T-0075]]
  for the modules; the platforms and layout can land first with the
  shell-drawn tables.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 — platforms, layout and README (modules still to come)

Done, apart from the modules (HLIN-T-0074, HLIN-T-0075):

- **`demo/hlin-collab.toml`** gains `checklist` (8083) and `feed` (8084),
  both `hlin-token`.
- **`angreal demo up --with collab`** now builds `hlin`,
  `hlin-sample-checklist` and `hlin-sample-feed` (not
  `hlin-sample-platform`), starts both platforms with `--shell-keys` pointing
  at the shell, waits for their manifests, starts the shell, then publishes
  the surface. `demo down` (and `up`, first) stops them with the rest.
  `_build_modules()` in `.angreal/task_demo.py` is the hook where each
  module's build goes; it does nothing yet, so criterion 1 stays unticked.
- **Seeding the layout, without a backdoor.** `up` signs in as Alice exactly
  as a browser would (`/auth/login` → Dex's form, posted → `/auth/callback`,
  cookie kept by a `urllib` cookie jar), waits until `/api/panels` offers
  `checklist/items` and `feed/posts`, then creates (or, on a later `up`,
  finds by title) Alice's layout **The team** and `PUT`s it `published` with
  the checklist at x 0 and the feed at x 6, 6×6 each, `list = ["team"]`.
  Re-running `up` reuses the same layout, and the `PUT` makes it her most
  recently changed one, which is what `home` opens for her.
- **Browser test** `e2e/tests/collab.spec.js`, run by `angreal e2e signin`
  after `signin.spec.js` (and skipping itself elsewhere, like signin): Alice
  signs in and lands on `/s/{id}` with no navigation; `home` is `published`
  with exactly the two panels; both are `ready`, drawn as `<table>`s side by
  side, with "Book a room for Thursday's review" and the feed's welcome post.
  Carol, in another context, opens the same `/s/{id}`: the checklist panel is
  `unavailable` with "you do not have access to this panel" and none of the
  team's items; the feed is `ready` with the posts.
- **README**: "Signing in as somebody: the collaborative demo" tells the
  story so far.

Runs: `angreal e2e signin` 5 passed (4 signin + 1 collab);
`angreal check all` passes; `angreal test all` 642 passed, 0 failed,
3 ignored.

**Question: what should a first-time person land on?** `home`
(`crates/hlin/src/layouts.rs`) returns the caller's most recently changed
*own* layout and, if they own none, creates an empty personal "My surface".
It never looks at published layouts except on a read-only shell. So Alice
lands on The team because she owns it, but a first-time Bob or Carol lands on
an empty surface of their own and has to be given the link (the README says
so). Not changed here, since it is shell behaviour.

Smallest change proposed: in `home`, when the principal owns no layouts and
something is published, return the most recently changed published layout
(read-only to them, `editable: false`, forkable) instead of creating one;
create "My surface" only when nothing is published. That is the read-only
branch's rule applied to someone with nothing of their own, and it also stops
`home` writing a row for everyone who merely signs in. Once they fork or
create a layout, `home` returns theirs as now. The e2e would then assert
Carol lands on The team from `/` rather than opening it by link.

### 2026-09-24 — where a newcomer lands: decided

The owner kept today's behaviour: someone who owns no surface lands on an
empty one of their own, and reaches published surfaces by link or gallery.
`home` is unchanged. So Alice, who published "The team", lands on it; Bob and
Carol open it by its link, which the README already says. The "what a person
lands on" criterion is met as decided, not as first written. Still open: the
module build step, which waits on [[HLIN-T-0074]] and [[HLIN-T-0075]].
