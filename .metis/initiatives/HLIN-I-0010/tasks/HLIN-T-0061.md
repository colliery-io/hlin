---
id: sign-in-to-the-demo-through-dex
level: task
title: "Sign in to the demo through Dex"
short_code: "HLIN-T-0061"
created_at: 2026-09-24T23:21:06.464152+00:00
updated_at: 2026-09-25T00:28:20.381989+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# Sign in to the demo through Dex

## Parent Initiative

[[HLIN-I-0010]]

## Objective

A person opens the demo shell, is sent to a real identity provider, signs in
as one of three named people, and lands on a surface as themselves. This is
the first time the demo runs the `oidc` authenticator at all.

## Acceptance Criteria

## Acceptance Criteria

- [x] Dex in `docker-compose.yml`, configured by `demo/dex.yaml`: issuer
      `http://127.0.0.1:5556/dex`, static users `alice@example.com`,
      `bob@example.com` and `carol@elsewhere.org` (password `password`), and a
      static client `hlin` whose redirect URI is the shell's callback
- [x] `demo/hlin-collab.toml`: the shell on `oidc` against Dex, secret from
      the environment, cookie not `Secure` (plain http on loopback). No
      platforms yet: `checklist` and `feed` arrive with their own tasks
- [x] An angreal way to bring it up (Postgres, Dex, the shell) that sets the
      client secret for both sides, alongside the existing `demo up`, not
      replacing it
- [x] Signing in as each of the three lands them on the shell with their own
      name, and signing out ends the session
- [x] A browser test that signs in through Dex's form and asserts the name
      shown, and a check that the principal carries email

## Implementation Notes

- Depends on [[HLIN-T-0060]]: without it the shell refuses Dex's http issuer
  and the principal has no email.
- Dex's static passwords carry no groups; decision 3 means none are needed.
- Mind `e2e-frontend-must-match`: the collaborative flavour needs its own
  frontend expectation, or interaction tests fail confusingly.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan (task 2). Not started.

### 2026-09-24 — done

- **Dex** in `docker-compose.yml` (`ghcr.io/dexidp/dex:v2.44.0`, container
  `hlin-dev-dex`, bound to `127.0.0.1:5556`, behind a `collab` profile so
  `db up` and the standard demo never start it). `demo/dex.yaml`: issuer
  `http://127.0.0.1:5556/dex`, memory storage, approval screen skipped, the
  three static users (bcrypt from `htpasswd -bnBC 10 "" password`, `$2y`
  rewritten to `$2a`; the command is in the file), usernames `Alice`, `Bob`,
  `Carol` (Dex issues them as `name`), static client `hlin` with
  `secretEnv: HLIN_DEMO_OIDC_SECRET` and redirect
  `http://127.0.0.1:8080/auth/callback`.
- **`demo/hlin-collab.toml`**: `oidc` against Dex, secret from
  `HLIN_DEMO_OIDC_SECRET`, `public_url = http://127.0.0.1:8080`, cookie not
  `Secure`, scopes without `groups`, no platforms.
- **`angreal demo up --with collab`**: a flavour like `aurora`. Generates the
  secret (unless already set) and hands it to Dex and the shell, starts Dex
  with `--force-recreate` so a stale Dex cannot hold an old secret, waits for
  discovery, starts the shell, and skips the sample platforms and the panels
  wait. `demo down` (and `up`, before starting) stops Dex, touching only that
  service so a database started from another checkout is left alone.
- **Name and sign-out in the shell.** Nothing showed who was signed in and
  nothing could sign out, so: `/api/config` gains `sign_out` (the logout path
  under `oidc`, null otherwise; `LOGOUT_PATH` constant), `ClientConfig` gains
  `sign_out` and `PrincipalSummary` `email` (both defaulted), and the bar shows
  the principal's name (or `sub`) and, where offered, a **Sign out** button
  that POSTs and reloads. Server tests assert `sign_out` under both `oidc` and
  `anonymous`.
- **Browser test** `e2e/tests/signin.spec.js`, run by the new
  `angreal e2e signin`: for each of the three people, goes to the shell, is
  sent to Dex's form, signs in, asserts the name in the bar,
  `principal.name`, `principal.email` and `sign_out` from `/api/config`, signs
  out, is back at Dex's form and `/api/config` is 401; plus a replayed
  pre-sign-out cookie being refused. It skips itself against a shell that does
  not sign people in, and `angreal e2e test` now refuses (with a pointer)
  against one that does, which keeps `e2e-frontend-must-match` from biting.
- README: a pointer from the oidc section and a "Signing in as somebody"
  section under the demo.

**HLIN-T-0060's gap is closed.** The browser test reads `principal.email` after
a real Dex sign-in, i.e. from the `email` claim of an id token Dex signed, and
it matched for all three users.

Found along the way: angreal loads every task file into one namespace, so a
`_run` helper in `task_e2e.py` was silently replaced by `task_tests.py`'s
(renamed `_playwright`); and angreal exits a failing task without flushing
Python's stdout, so the new refusal messages print with `flush=True`.

Runs: `angreal e2e signin` 4 passed against `--with collab`; `angreal e2e
test` 23 passed / 10 skipped against `--with aurora` (the signin tests skip
there); `angreal check all` and `angreal test all` pass.

Left: nothing for this task. `checklist` and `feed` join `hlin-collab.toml`
with their own tasks.
