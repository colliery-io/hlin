---
id: the-front-end-stops-offering-what
level: task
title: "The front end stops offering what cannot succeed"
short_code: "HLIN-T-0051"
created_at: 2026-09-09T00:30:35.836724+00:00
updated_at: 2026-09-09T00:30:35.836724+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# The front end stops offering what cannot succeed

## What

`/api/config` tells the browser the shell is read-only, and the front end stops
offering Edit rather than offering it and failing at the end.

## Shape

- Add `"read_only": bool` to the `/api/config` payload, from the strategy.
- `crates/hlin-ui` hides the Edit control, the panel picker, and anything else
  whose only outcome is a refused write.
- Nothing pack-specific. This is chrome, so no design pack learns a new word
  and `vendor/aurora-leptos` is untouched — the constraint is that Hlin owns
  nothing Aurora-specific and packs owe Hlin no vocabulary for this.

## Done when

- `/api/config` reports `read_only` correctly for all four strategies.
- With it true, no control that writes is on screen, in the demo pack and in
  Aurora, without either pack changing.
- A browser test asserting Edit is absent under `anonymous` and present under
  `dev`.

## Status Updates

- 2026-09-09: Done, server and browser halves. `read_only` added to
  `ClientConfig` (`#[serde(default)]`, so an older browser assumes it may write
  and is corrected by a 403 rather than hiding composition from a shell that
  offers it) and to `/api/config`. In `hlin-ui`, the Edit button is absent
  under read-only rather than disabled — a disabled control says "not now", and
  there is no later here — and the empty-surface message stops telling people
  to press a button that is not on screen.
- No design pack changed. `vendor/aurora-leptos` untouched.
