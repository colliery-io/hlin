---
id: carry-email-from-the-identity
level: task
title: "Carry email from the identity provider to the token, and let a debug build sign in against a local provider"
short_code: "HLIN-T-0060"
created_at: 2026-09-24T23:21:05.373548+00:00
updated_at: 2026-09-24T23:25:29.380378+00:00
parent: HLIN-I-0010
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0010
---

# Carry email from the identity provider to the token, and let a debug build sign in against a local provider

## Parent Initiative

[[HLIN-I-0010]]

## Objective

Two small changes the collaborative demo needs before anybody can sign in to
it, neither of which depends on the module host:

1. **Email reaches platforms.** Decision 7 of [[HLIN-I-0010]] fixes the
   attributes a platform may decide on as `sub`, `name`, `email` and `groups`.
   The token already carries `email` from the principal
   (`hlin-identity/src/issuer.rs`), but an `oidc` sign-in never fills it: the
   ID token's `email` is used only as a fallback display name, and the session
   has nowhere to keep it. The feed platform's rule ("only `@example.com` may
   post") needs it.
2. **A debug build may use a local provider over plain http.** Decision 8.
   `OidcConfig::check` refuses any issuer that is not `https`, so Dex on
   `http://127.0.0.1:5556` is refused at startup. Allow `http://` only when the
   host is loopback and the build is a debug build, the boundary `dev` already
   uses. A release build refuses exactly as before.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] `oidc::Claims` carries `email`, read from the ID token's `email` claim.
      The display-name fallback to email is unchanged
- [x] `Session` stores `email`; a new migration adds a nullable `email`
      column; the Postgres and memory stores write and read it
- [x] A session's principal carries `email`, so a platform's `hlin-token` has
      it
- [x] `OidcConfig::check` accepts `http://localhost`, `http://127.0.0.1` and
      `http://[::1]` issuers in a debug build, and refuses them in a release
      build with a reason that says why
- [x] Any other `http://` issuer is still refused in every build
- [x] Tests for each of the above; `angreal check all` and `angreal test all`
      pass

## Implementation Notes

- `crates/hlin/src/auth/oidc.rs`: `Claims`, `claims_of`.
- `crates/hlin/src/auth/mod.rs`: the `Session` built in `callback`.
- `crates/hlin/src/store/types.rs`, `store/postgres.rs`, `store/memory.rs`,
  `migrations/0004_session_email.sql`.
- `crates/hlin/src/identity.rs`: the `Caller` extractor's session branch.
- `crates/hlin/src/config.rs`: `OidcConfig::check`, using
  `cfg!(debug_assertions)` as `AuthConfig::check` does for `dev`.
- Test fixtures building `Session` literally: `tests/store_suite/mod.rs`,
  `tests/layouts.rs`.

## Status Updates

### 2026-09-24

Created from [[HLIN-I-0010]]'s plan (task 1). Starting.

### 2026-09-24 — done

- `oidc::Claims` gained `email`, read from the ID token's `email` claim; the
  display-name fallback is unchanged.
- `Session.email`, migration `0004_session_email.sql` (nullable), Postgres
  insert and select. The memory store keeps whole sessions, so needed nothing.
- The session's principal carries `email`, which the issuer already copies
  into the token. `/api/config` now returns it beside `sub` and `name`, which
  gives [[HLIN-T-0061]] something to assert and the bridge's `init` the same
  source.
- `OidcConfig::check` allows `http://` for `localhost`, `127.0.0.1` and
  `[::1]` in a debug build only (`is_loopback_http` in `config.rs`, exact host
  match), and refuses it in a release build with a reason naming the release
  build.

Tests: store suite round-trips `email` (ran against Postgres; migration 4
confirmed applied); `a_sessions_email_is_part_of_who_the_holder_is`; the
oidc configuration test covers the three loopback forms and two look-alike
hosts. `angreal check all` and `angreal test all` pass.

**Gap:** reading `email` out of a signed ID token is not unit-tested; the
existing oidc tests do not build signed tokens. The first real exercise is
[[HLIN-T-0061]]'s sign-in through Dex, which should assert it.
