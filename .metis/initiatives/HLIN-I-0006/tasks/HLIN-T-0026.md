---
id: implement-the-authenticator
level: task
title: "Implement the authenticator strategies: trusted-header, then oidc"
short_code: "HLIN-T-0026"
created_at: 2026-09-08T01:56:05.876337+00:00
updated_at: 2026-09-08T10:27:10.526064+00:00
parent: HLIN-I-0006
blocked_by: []
archived: false

tags:
  - "#task"
  - "#tech-debt"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0006
---

# Implement the authenticator strategies: trusted-header, then oidc

## Objective

Make a request to the shell carry a real principal. `AuthConfig` has one
variant, `Dev`, and `Config::principal()` returns whatever the TOML names, for
every request, with no middleware in front of any handler. HLIN-S-0005
specifies three authenticator strategies; one exists.

Finding 1 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt — the one piece of the design not yet built

### Priority
- [x] P0 - Critical: the shell cannot be deployed without it

### Technical Debt Impact
- **Current Problems**: `GET`/`PUT`/`DELETE /api/layouts/{id}`, `/api/stream`,
  `/api/options` are open and act as `u_dev`. Ownership (`is_editable_by`),
  visibility (`is_visible_to`), per-principal surfaces and dedup, and platform
  authorization are all implemented, tested, and vacuous — everyone is the same
  person.
- **Benefits of Fixing**: HLIN-A-0004 and HLIN-A-0007 start doing what they say.
  [[HLIN-T-0028]] stops running as the dev principal.
- **Risk Assessment**: Any exposure of the shell beyond a laptop exposes every
  layout and every platform the shell can reach, as the configured principal.

## Acceptance Criteria

## Acceptance Criteria

- [x] `trusted-header` authenticator: reads the configured header, refuses with
      401 when absent or empty, never falls through to `Dev`
- [x] `oidc` authenticator per HLIN-S-0005, with session establishment at the
      shell's own origin. The three questions it was waiting on — session
      storage, cookie security, redirect handling — are answered in the status
      update below rather than having been decided quietly inside a loop.
- [x] Authentication is enforced before any handler runs. An extractor rather
      than a tower layer, which is stronger than what was asked for: a handler
      that needs a principal says so in its signature and cannot be written
      without one.
- [x] `Dev` refuses to start in a release build, the way `hlin-identity` refuses
      `dev-identity`
- [x] Layout ownership tests run against two distinct principals and prove the
      forbidden path

## Implementation Notes

### Technical Approach
Where: `crates/hlin/src/config.rs:82` (`AuthConfig`), `config.rs:346`
(`principal()`), `server.rs:32` (`router()`). Replace `state.config.principal()`
at every call site with an extractor reading the principal the layer attached.
`trusted-header` first: the smallest real strategy, and it unblocks a
deployment behind an existing proxy.

### Dependencies
None. Unblocks every ownership rule and [[HLIN-T-0028]]'s threat model.

### Risk Considerations
Every handler currently calls `principal()`; a missed call site silently keeps
the dev principal. Make the old function private or remove it so the compiler
finds them.

## Status Updates

### 2026-09-08 — `trusted-header` done; `oidc` separated, not skipped

**The shape change is the substance.** `Config::principal()` took no arguments
and returned the same person for every request. It is now
`principal_from(&HeaderMap) -> Option<Principal>`, and handlers take a `Caller`
extractor — so a request carrying no principal is refused with 401 *before* any
handler runs.

There is no way to write a handler that forgets to authenticate, because there
is nothing left to forget: the principal only exists as an argument. Making the
old method impossible was what found all eleven call sites, exactly as the
ticket predicted.

`None` deliberately does not mean "fall back to the configured principal". A
strategy that silently degraded to `dev` when a header was missing would be a
strategy nobody could tell was broken.

**`Dev` refuses to start in a release build**, the same rule `hlin-identity`
applies to its own bypass, for the same reason: a flag that can be set at
runtime is a flag that will be set at three in the morning to make something
work.

**`trusted-header` must acknowledge what it trusts.** Its whole security rests
on nothing reaching the shell except through the proxy — a header is trivially
forged by anything that can connect directly — and that failure is silent from
the inside. So `acknowledge_proxy_required = true` is required, and the refusal
names it, in the same shape `static-bearer` already uses.

**Ownership is no longer vacuous**, and there is now a test that could not have
been written before: `ada` creates a layout over the real router, `grace` is
refused with 403 on the write, and a request with no header at all gets 401.
Two distinct principals in one test — the thing every rule in HLIN-A-0007 was
built against and none could exhibit.

307 Rust tests (up from 303), 23 browser tests, walkthrough holds.

### `oidc`, and why it is not here

Not started, and separated rather than rushed. It needs decisions that are not
mine to make quietly inside a loop:

- **Session storage.** Cookie-only, or server-side sessions in the store? The
  second means a new table, a sweeper, and a decision about what a restart does
  to everyone's session.
- **Cookie security.** `SameSite`, `Secure`, rotation, and how logout works —
  each of which is a promise to the deployment.
- **Redirect handling.** The callback URL is deployment-specific, and getting
  the state/nonce round trip wrong is a real vulnerability rather than a bug.

`trusted-header` unblocks a deployment behind an existing authenticating proxy
today, which is what the ticket said to reach for first. `oidc` should be its
own task with those three questions answered before any code.

Suggest splitting this task: mark the `trusted-header` half done and open
`oidc` separately.

### 2026-09-08 — `oidc` built, with the three questions answered

**Session storage: rows in Postgres**, as decided. Migration `0003_sessions.sql`
adds `sessions` and `pending_logins`; the `Store` trait gains six methods and
both implementations satisfy the same suite.

Rows rather than a signed cookie, because a self-contained cookie cannot be
withdrawn: signing out clears the browser's copy and nothing else, and there is
no answer to "revoke this person now" short of rotating a key and ending
everyone's session at once. A restart signs nobody out, which is the behaviour
to want — the alternative logs out an organisation because a shell was
redeployed.

**The cookie's value is never stored.** A session is keyed by its SHA-256, so a
dump of the table is a set of useless hashes rather than a set of live sessions.
The shell has the value on every request and can hash it, so storing it buys
nothing at all and costs the whole table's worth of access.

**Cookie policy**: `HttpOnly` always — nothing in the frontend has any use for
the value, and a session a script can read is one any script on the page can
take. `Secure` by default, and a shell served over `https` whose cookie is not
`Secure` is refused at startup. `SameSite=Lax`, and `Strict` is *refused*
rather than merely defaulted away from: the provider returns the browser by a
top-level redirect, which `Strict` treats as cross-site and strips the cookie
from, so a person would arrive back signed in and apparently not, forever. That
is the kind of failure worth a startup refusal, because from the inside nothing
looks wrong.

**Redirect handling**: `state`, `nonce` and the PKCE verifier are written down
server-side and the callback deletes the row it claims, so a replayed callback
finds nothing. The `next` parameter is reduced to a path on this shell —
`//host` is a path to a browser and another origin to a person reading it — for
the reason the login route is exactly where open redirects are found.

**`Option` was the wrong return type and hid the real change.**
`principal_from` answered `Option<Principal>`, and a session is neither. It now
answers `Asking::{Known, Nobody, Session}`, so the compiler found every caller
and none of them could quietly treat "ask the database" as "nobody" — which
would have compiled and refused every request under `oidc`.

**A 401 now says where to go, where there is anywhere.** Two shells answer the
same status to the same request and a browser must do opposite things about it:
sign in, or tell the person that the proxy in front of the shell is broken. The
status cannot carry that, so the body does — `{"login": "/auth/login"}` under
`oidc`, `{"login": null}` otherwise — and the frontend follows it rather than
guessing, which under `trusted-header` would be a redirect loop against a shell
with no login at all.

**Not built, deliberately**: the session keeps the subject, name and groups the
id token asserted, not the provider's access or refresh tokens. The two
credentialers that would need them — `forward-bearer` and `token-exchange` —
are not built either, so keeping tokens would mean holding a credential for
every signed-in person against a use nothing makes yet. HLIN-S-0005 now says so
where it used to imply otherwise.

319 Rust tests (up from 313), 24 browser tests, walkthrough holds. Both
acceptance criteria are met; the task is done.
