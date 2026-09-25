---
id: sign-in-to-the-demo-through-dex
level: task
title: "Sign in to the demo through Dex"
short_code: "HLIN-T-0061"
created_at: 2026-09-24T23:21:06.464152+00:00
updated_at: 2026-09-25T00:02:33.988500+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


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

- [ ] Dex in `docker-compose.yml`, configured by `demo/dex.yaml`: issuer
      `http://127.0.0.1:5556/dex`, static users `alice@example.com`,
      `bob@example.com` and `carol@elsewhere.org` (password `password`), and a
      static client `hlin` whose redirect URI is the shell's callback
- [ ] `demo/hlin-collab.toml`: the shell on `oidc` against Dex, secret from
      the environment, cookie not `Secure` (plain http on loopback). No
      platforms yet: `checklist` and `feed` arrive with their own tasks
- [ ] An angreal way to bring it up (Postgres, Dex, the shell) that sets the
      client secret for both sides, alongside the existing `demo up`, not
      replacing it
- [ ] Signing in as each of the three lands them on the shell with their own
      name, and signing out ends the session
- [ ] A browser test that signs in through Dex's form and asserts the name
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
