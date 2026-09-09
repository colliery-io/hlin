---
id: run-without-an-identity-provider
level: initiative
title: "Run without an identity provider"
short_code: "HLIN-I-0008"
created_at: 2026-09-09T00:30:14.683860+00:00
updated_at: 2026-09-09T00:30:14.683860+00:00
parent: hlin
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/ready"

exit_criteria_met: false
estimated_complexity: S
initiative_id: run-without-an-identity-provider
---

# Run without an identity provider

## Problem

A release build of Hlin cannot start without either an authenticating proxy or
an OpenID Connect provider. `dev` is refused outside a debug build because it
makes every request the same person; `trusted-header` requires a proxy;
`oidc` requires a provider, a client registration, a secret and a public URL.

An open ecosystem has none of those and no reason to acquire them. Hlin is
unusable there today, and that is a deployment the vision is supposed to serve.

## Decision

[[HLIN-A-0012]]. A fourth strategy, `anonymous`:

- Usable in a release build.
- Every visitor gets their own principal from a cookie the shell sets, because
  surfaces are keyed `{surface_id}:{principal.sub}` and a shared principal
  would have one visitor's click move everybody else's charts.
- Every write is refused, and the browser is told so it stops offering Edit.
  `anonymous` plus writes is `dev`, which is refused for that reason.

## Success condition

A release build, given a configuration naming no identity provider and no
proxy, starts; a browser with no credentials opens a published surface, drives
its controls, and gets its own time range; a second browser is unaffected by
the first; and every attempt to create, edit, delete or fork a layout is
refused with a reason. Proven against the real image, not asserted.

## Out of scope

- Composing on an anonymous instance. Layouts are authored elsewhere against
  the same database. Anonymous authoring means any visitor can delete what
  every other visitor came to see.
- A read-only mode under `oidc` or `trusted-header`. That is authorisation and
  belongs in its own decision.

## Status Updates

- 2026-09-09: Decomposed from [[HLIN-A-0012]]. Six tasks, T-0049..T-0054.
- 2026-09-09: All six complete. The success condition was checked against a
  release binary rather than asserted — see the table in [[HLIN-T-0054]]. 393
  Rust tests passing, clippy and fmt clean, the demo walkthrough holds, and the
  browser suite is 23 passed / 6 skipped against the demo shell plus 5 passed
  against the open one.
- Two defects found on the way, both pre-existing and both silent:
  `PrincipalSummary::name` was a bare `String` against a shell that sends
  `null`, so `/api/config` failed to parse for any principal without a display
  name and the browser quietly kept its built-in defaults; and a release-only
  `unused_imports` warning a debug clippy run could not see.
- Awaiting review. Not transitioned to completed.
