---
id: a-platform-may-need-no-credential
level: task
title: "A platform may need no credential at all"
short_code: "HLIN-T-0052"
created_at: 2026-09-09T00:30:38.396040+00:00
updated_at: 2026-09-09T00:30:38.396040+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# A platform may need no credential at all

## What

A `none` credentialer, so a platform can be registered with nothing but a base
URL.

In an open ecosystem the platforms are public and want no credential. Today the
three strategies are `forward-session`, `hlin-token` and `static-bearer`, and
the shell must pick one. `hlin-token` works — a public platform ignores a
header it does not read — but it mints and signs a token per request for
nobody, and it makes the configuration claim a relationship that does not
exist.

## Shape

- `CredentialConfig::None`, `strategy = "none"`.
- `headers()` returns nothing.
- `check()` has nothing to object to — but the *manifest* may. A platform that
  declares it requires a credential and is configured with `none` is a
  misconfiguration worth naming at startup if the manifest says so.

## Done when

- A platform configured `auth = { strategy = "none" }` is polled with no
  Authorization header, and its panels render.
- `hlin check` names it.
- The sample platform gains a mode that accepts unauthenticated requests, so
  this is proven rather than asserted.

## Status Updates

- 2026-09-09: Done. `CredentialConfig::None`, `strategy = "none"`: no headers,
  nothing to check, collapses no principals because it carries none.
- 2026-09-09: Proven end to end in T-0054. A sample platform on 8083 with
  `--auth open`, configured `auth = { strategy = "none" }`, registered
  `reachable: true` with 14 panels and no Authorization header sent.
