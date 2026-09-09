---
id: prove-it-against-the-real-image
level: task
title: "Prove it against the real image, with two browsers"
short_code: "HLIN-T-0054"
created_at: 2026-09-09T00:30:43.433638+00:00
updated_at: 2026-09-09T00:30:43.433638+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# Prove it against the real image, with two browsers

## What

Prove the success condition against the real image, with two browsers, rather
than asserting it from unit tests.

## The claims

1. A release build starts with a configuration naming no provider and no proxy.
2. A browser with no credentials opens a published surface and sees data.
3. It drives a control — a filter, a time range — and the drawing changes.
4. A *second* browser, with its own cookie, is unaffected by the first. This is
   the claim that fails if the per-visitor identity is wrong, and it is the one
   a single-browser test cannot make.
5. Create, replace, delete and fork are each refused with a reason.
6. No Edit control is on screen.

## Shape

The image, Postgres, and a layout composed beforehand by a shell running a real
authenticator against the same database — which is also the documented way an
operator seeds an open instance, so this exercises the story it tells.

Compose and verify the link **last**, after everything else has run.

## Done when

Every claim above is checked and its result recorded here, with the numbers and
the commands, so it can be re-run.

## Status Updates

## Result — 2026-09-09

Proven against `target/release/hlin` (not a debug build), a real Postgres, and
three sample platforms. The open shell ran on 8090 from
`/tmp/catest/anon/shell.toml`; the composing shell was the demo on 8080.

| # | Claim | Result |
|---|-------|--------|
| 1 | A release build starts with no provider and no proxy | `listening on http://127.0.0.1:8090 platforms=3`. The same binary still refuses `dev`. |
| 2 | A browser with no credentials is answered, and named | `set-cookie: hlin_visitor=…; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000`, and the request that minted it already knew `sub=anonymous:…` |
| 3 | It sees a published surface with data | `home` returned "The open wall" (published, 2 panels); the stream went 4 loading → 54 ready |
| 4 | A second browser is unaffected by the first | Holding both streams open, A moved the window: **A saw generations 1 then 2; B saw only 1** |
| 5 | Every write is refused, with a reason | POST/PUT/DELETE/fork → 403 `error=read_only`, each with the detail text |
| 6 | No Edit control on screen | `button.mode` count 0, `.catalog` count 0 |

Also proven: **a platform reached with no credential at all** (T-0052). A third
sample platform on 8083 with `--auth open`, configured
`auth = { strategy = "none" }`, registered `reachable: true, panels: 14`.

`e2e/tests/open.spec.js` is the durable form of claims 2, 3, 4, 5 and 6 — five
tests, which skip unless `/api/config` reports the shell is read-only, so one
suite stays honest against two very different deployments:

```
HLIN_URL=http://127.0.0.1:8090 npx playwright test tests/open.spec.js
```

### What this found

`PrincipalSummary::name` was a bare `String` while the shell has always sent
`null` for a principal with no display name. So `/api/config` failed to
deserialise for every such principal — and a browser's only response to a
failed fetch is to keep its built-in defaults, silently. It affected
`trusted-header` with no `name_header` and `oidc` against a provider returning
no `name` claim; under `anonymous` it is every visitor, which is the only
reason it surfaced. Now `Option<String>`, with a regression test that parses a
real `/api/config` body into the browser's own type.

Also fixed: a release-only `unused_imports` warning on `MemoryStore`, invisible
to a debug-build clippy run.

### Reproducing

The open shell shares the composing shell's `issuer` and `key_path` — that is
not a workaround, it is what the chart already requires of two replicas, and
platforms verify tokens against one key set.

## Status Updates

- 2026-09-09: Done. Every claim checked; the table above is the record.
