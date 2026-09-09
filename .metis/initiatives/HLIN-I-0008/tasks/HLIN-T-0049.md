---
id: an-anonymous-authenticator-with-a
level: task
title: "An `anonymous` authenticator, with a principal per visitor"
short_code: "HLIN-T-0049"
created_at: 2026-09-09T00:30:30.605231+00:00
updated_at: 2026-09-09T00:30:30.605231+00:00
parent: HLIN-I-0008
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0008
---

# An `anonymous` authenticator, with a principal per visitor

## What

A fourth `AuthConfig` variant, `Anonymous`, allowed in a release build.

Each visitor gets their own `principal.sub` from a cookie the shell sets. This
is the whole point of the task: `surfaces.rs:48` keys a live surface
`{surface_id}:{principal.sub}`, so one shared anonymous principal means one
shared surface — a visitor clicking a filter would move the charts for
everybody else on the same page.

## Shape

- `AuthConfig::Anonymous { cookie: CookieConfig, name: Option<String> }`.
  Reuse `CookieConfig`; the scoping question is the same one `oidc` answers.
- `name()` returns `"anonymous"`.
- `check()` returns `Ok` in both debug and release. This is the strategy that
  is *safe* without a provider, unlike `dev`.
- `principal_from` cannot mint-and-set a cookie on its own — it takes headers
  and returns `Asking`. Either add an `Asking` variant for "no identity yet,
  make one", or resolve it in the extractor where a response can carry a
  `Set-Cookie`. Prefer the latter: `Asking` describes what the headers say, and
  minting is not that.
- `routes()` stays empty for `anonymous` — there is nothing to sign in to.

## Done when

- A config naming `strategy = "anonymous"` loads and `hlin check` reports it.
- A request with no cookie is answered *and* carries `Set-Cookie`.
- A request carrying the cookie resolves to the same `sub` as the response that
  set it.
- Two different cookie values are two different principals, so two surfaces.
- Unit tests for each of the above.

## Status Updates

- 2026-09-09: Done. `AuthConfig::Anonymous { cookie, name }` in `config.rs`;
  `check()` allows it in a release build and refuses a nameless cookie;
  `principal_from` returns `Known` with `sub = "anonymous:{cookie}"`.
  Minting is a layer (`visitor.rs`) rather than the extractor, so
  `principal_from` stays a function of the headers: the layer writes the cookie
  into the request the handlers see *and* onto the response, so the first
  request of a visit is answered rather than refused. Tests: 10 in
  `tests/anonymous.rs`, including `two_visitors_are_two_people`, which is the
  claim a single-browser test cannot make.
