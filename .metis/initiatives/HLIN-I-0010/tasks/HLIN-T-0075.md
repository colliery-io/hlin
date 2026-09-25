---
id: the-feed-s-own-module
level: task
title: "The feed's own module"
short_code: "HLIN-T-0075"
created_at: 2026-09-25T00:39:02.766139+00:00
updated_at: 2026-09-25T01:36:21.618956+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The feed's own module

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 6 of [[HLIN-I-0010]]. The feed's UI as a module.

## Acceptance Criteria

## Acceptance Criteria

- [x] A Leptos module for the feed, built with the SDK and the shared kit
- [x] Posts newest first with author and time, a compose box, edit and delete
      on each post; refusals in the feed's own words
- [x] Announces `changed` after a write, refetches on `changed`
- [x] The manifest's `posts` panel gains `ui`; the `table` fallback stays
- [x] Built by the same angreal path as [[HLIN-T-0074]]

## Implementation Notes

- Blocked on [[HLIN-T-0066]] and [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 (implemented)

Built with [[HLIN-T-0074]], in the same shape so either can be copied alone.

**What.**

- `crates/hlin-sample-feed/module/`: `hlin-sample-feed-module`, Leptos 0.8,
  Trunk into `module/dist/`, drawn with Aurora (`Textarea`, `Button`,
  `Alert`, `Loading`, `Empty`). `api.rs` (6 native tests: newest first
  whatever order arrives, the feed's words, the shell's reason never shown,
  times as a person says them, ids that cannot add a segment); `app.rs` the
  view.
- Posts newest first (sorted by `posted_at` in the module, not trusted to the
  API's order), each with author, a relative time ("5 min ago", the date
  after a day; the exact time in `title`) and "edited" when it was, a compose
  box, and Edit/Delete on every post. The feed tells its module nothing about
  who may do what (and has no viewer id to compare), so both are offered on
  every post and the feed's refusal is shown as it wrote it ("Only the author
  can edit this post", "Only people at example.com can post here"). A refused
  edit keeps the person's words in the box. `changed("posts", {})` after a
  successful write; refetch on `context`, on a `changed` for `posts`, and
  after every write whatever its answer. Retry of a network failure resends
  with the same key.
- Platform: `src/module.rs` serves `/ui/posts/{file}`; manifest 1.1.0 has
  `assets = /ui/` and `ui.entry = /ui/posts/index.html` on `posts`, keeping
  the `table`. `Config` gained `module: ModuleFiles`; `--module-dir`.
- Built by `_build_modules()` beside the checklist's.

Same decision on serving (read from disk at start) and the same fixes found
on the way as [[HLIN-T-0074]] records: the shell's `/m/` CORS header, the
`boot.js` loader, the executor, and memo tracking.

**Checks.** As [[HLIN-T-0074]]: `angreal check all` clean, `angreal test all`
699 passed, 0 failed, 3 ignored; `angreal e2e signin` 9 passed, including
Alice posting (newest first, as Alice), Bob's edit of it refused in the feed's
words with the post unchanged, and Carol's post refused in the feed's words.

**Not verified here**: another browser refetching on the relayed `changed` is
HLIN-T-0067's to deliver and HLIN-T-0077's to assert.
