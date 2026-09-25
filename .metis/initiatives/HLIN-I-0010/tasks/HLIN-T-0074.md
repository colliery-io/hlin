---
id: the-checklist-s-own-module
level: task
title: "The checklist's own module"
short_code: "HLIN-T-0074"
created_at: 2026-09-25T00:39:01.720914+00:00
updated_at: 2026-09-25T01:36:20.848015+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# The checklist's own module

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Task 5 of [[HLIN-I-0010]]. The checklist ships its own UI as a module built
with the SDK ([[HLIN-T-0069]]), hosted by the shell ([[HLIN-I-0011]]).

## Acceptance Criteria

## Acceptance Criteria

- [x] A Leptos module in the checklist crate (or beside it), built with the
      SDK and the shared kit, served under the platform's `assets` prefix
- [x] Shows the chosen list's items with a checkbox to cross off, inline edit,
      delete, and an add field; a list picker using `set-param`
- [x] Refusals shown in the checklist's own words from the platform's answer
- [x] Announces `changed` after a write, and refetches on `changed` and on
      `context`
- [x] The manifest's `items` panel gains `ui`; the `table` fallback stays
- [x] An angreal way to build it that `demo up --with collab` uses

## Implementation Notes

- Blocked on [[HLIN-T-0066]] and [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan. Not started.

### 2026-09-24 (implemented)

**What.**

- `crates/hlin-sample-checklist/module/`: a Leptos 0.8 crate
  (`hlin-sample-checklist-module`, `publish = false`), built by Trunk into
  `module/dist/`. `api.rs` holds the requests and how answers are read (8
  native tests: the viewer hint, the platform's words, the shell's refusals
  never shown as the platform's, retry only for network failures, ids that
  cannot add a path segment); `app.rs` the view. Drawn with Aurora
  (`colliery-io-aurora` 0.2: `Button`, `TextInput`, `Alert`, `Loading`,
  `Empty`, and its `cl-*` classes for the checkbox row and picker), its tokens
  mapped onto whatever `--hl-*` tokens the shell sends, falling back to
  Aurora's own. `ready(Some("aurora@0.2"))`.
- Behaviour: items with a checkbox, inline edit (Enter saves, Escape
  cancels), delete, an add field; edit and delete offered only where the
  list's `viewer` hint says the platform would allow (author or owner), and
  the platform's refusal shown in its words if one comes anyway. A list
  picker from `/api/lists` that calls `set_param("list", …)` and also keeps
  the pick locally until the shell's next `context`, so it works before
  HLIN-T-0067 and defers to the shell after. A surface-chosen list the viewer
  is not on reads "Pick a list", with the checklist's refusal below. Every
  write is an `Attempt`; a network failure offers "Try again", which resends
  with the same idempotency key. After any write the list is fetched again
  (nothing is changed on screen ahead of the platform); after a successful
  one `changed("items", {list: [id]})` is sent. Refetches on `context` and on
  a `changed` for `items` naming this list or none. Rows are keyed on their
  whole content, so an edit in progress survives someone else's change.
- Platform: `src/module.rs` serves the build under `/ui/items/{file}`;
  manifest 1.1.0 declares `assets = /ui/` and `ui.entry =
  /ui/items/index.html` on `items`, which keeps `table`/`records.v1`.
  `--module-dir` (default `module/dist` beside the crate).
- `_build_modules()` in `.angreal/task_demo.py` runs `trunk build` for both
  modules before the platforms start, and `up` passes each its
  `--module-dir`. `crates/*/module` joined the workspace members, so the
  modules are checked, linted and unit-tested natively with everything else.

**Decision: read from disk at start, not embedded.** Embedding would make
`cargo check`/`test` of the workspace need Trunk and a finished wasm build
first, everywhere, to test a server that does not care what the module says.
Read at start into memory (served by exact name, so no request path touches
the filesystem), a clean checkout builds and tests without Trunk, and a
platform with no module built still starts; the shell finds the entry missing
and draws the table fallback. Hashed Trunk output is sent `immutable`; the
entry and `boot.js` `no-cache`.

**Found, and fixed on the way** (both modules hit them; neither was
reachable before a Trunk-built module was hosted):

- *The shell's `/m/` answers could not be read by the frame.* A sandboxed
  frame's origin is `null`, so its module script, `modulepreload`, stylesheet
  with integrity and the `.wasm` fetch are all CORS requests, and `/m/` sent
  no `Access-Control-Allow-Origin`. Every Trunk module failed before its first
  line; the probe module never noticed because it is a classic script and
  fetches nothing. `crates/hlin/src/modules/assets.rs` now sends
  `Access-Control-Allow-Origin: *` on every `/m/` answer (and never
  `-Credentials`), with a test in `crates/hlin/tests/assets.rs`. `*` reveals
  nothing: these are files the shell fetched without a credential and serves
  without a session to anyone with the URL.
- *Trunk's loader is an inline script*, which the module CSP refuses. Each
  module's `Trunk.toml` sets `pattern_script` to a tag naming `boot.js` (a
  file, copied beside the build) with the hashed glue and `.wasm` names as
  data attributes, and `public_url = "./"` so the build works under whatever
  `/m/{platform}/` the shell chooses. The stylesheet link carries
  `crossorigin="anonymous"` so Trunk's SRI check can read it.
- *`leptos::task::spawn_local` before anything is mounted panics* ("before a
  global executor was initialized"): `mount_to_body` is what starts Leptos's
  executor, and a module mounts only after `init`. `main` uses
  `wasm_bindgen_futures::spawn_local` for the wait. `hlin-module`'s crate-doc
  example has the same mistake (it is `no_run`, so nothing caught it); not
  changed here, worth fixing in the SDK.
- *A Leptos `Memo` that is only `.track()`ed is never computed*, so never
  hears of a change. Read with `.get()` instead; commented where it matters.

**Checks.** `angreal check all` clean; `angreal test all` 699 passed, 0
failed, 3 ignored. Against `angreal demo up --with collab`, `angreal e2e
signin`: 9 passed, including the new `e2e/tests/collab-modules.spec.js` (Alice
adds, ticks, edits in place and deletes an item in her module; Bob then sees it
ticked and is not offered its edit; Carol is shown the checklist's refusal and
picks her own list) and `collab.spec.js`, updated for modules (both panels
`data-module=ready`; Carol's checklist shows "Only members of Team can see or
change it, and you are not one." where it used to show the shell's
*unavailable*).

**Not verified here**: `changed` reaching another browser and the shell
honouring `set-param` are HLIN-T-0067's to deliver and HLIN-T-0077's to
assert; the module's side is built against the SDK and in place.
