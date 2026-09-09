---
id: chart-image-config-and-docs-offer
level: task
title: "Chart, image config and docs offer the open deployment"
short_code: "HLIN-T-0053"
created_at: 2026-09-09T00:30:41.150606+00:00
updated_at: 2026-09-09T00:30:41.150606+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# Chart, image config and docs offer the open deployment

## What

Make the open deployment configurable from the places people actually deploy
from.

## Shape

- `charts/hlin`: `config.auth.strategy: anonymous`, with the trusted-header
  acknowledgement and the oidc requirements not demanded of it. `hlin.validate`
  must not fail an anonymous install.
- `docker/hlin.toml`: show it, and say what it costs — nothing can be composed.
- `README.md`: the "Running it" section currently sends a reader to a proxy or
  an IdP. Offer the third door, and be honest that it is read-only.
- `CHANGELOG.md`.
- The release workflow renders the new shape and exercises any new refusal.

## Done when

- `helm template` renders an anonymous install and it needs no proxy
  acknowledgement and no oidc fields.
- The README's shortest path to a running release build needs no provider.
- `helm lint` clean; the workflow's render and refusal lists cover it.

## Status Updates

- 2026-09-09: Done. `config.auth.strategy: anonymous` in the chart, with an
  `anonymous.cookie` block; `hlin.validate` demands nothing of it — no proxy
  acknowledgement, no oidc fields — and the unknown-strategy message now names
  all three. `docker/hlin.toml`, `README.md` (a new "No identity provider"
  section, and the `dev` refusal now points at it), `CHANGELOG.md`, and the
  release workflow's render list.
- Proven rather than assumed: the chart-rendered config was extracted and fed
  to `hlin check`, which read it back as `strategy = "anonymous"` with a
  platform on `none`. A YAML template that renders is not the same as a TOML
  the shell can parse, and the `[auth.cookie]` nesting is exactly where an
  internally-tagged enum would have failed.
