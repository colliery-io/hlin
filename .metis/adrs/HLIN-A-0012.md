---
id: 001-an-identity-provider-is-not
level: adr
title: "An identity provider is not required: `anonymous` is a strategy, and it is read-only"
number: 3
short_code: "HLIN-A-0012"
created_at: 2026-09-09T00:29:35.217480+00:00
updated_at: 2026-09-09T00:29:35.217480+00:00
decision_date: 2026-09-09
decision_maker: Dylan Storey
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"

exit_criteria_met: false
initiative_id: NULL
---

# ADR-3: An identity provider is not required: `anonymous` is a strategy, and it is read-only

## Context

Three authenticators exist, and a release build can use two of them. `dev` is
refused outside a debug build because it makes every request the same person.
`trusted-header` needs a proxy that authenticates. `oidc` needs an identity
provider, a client registration, a secret, and a public URL that matches what
the provider was told.

So the shortest path to a running release build is: stand up Dex, or stand up a
proxy. Neither is a thing you do to look at something. An open ecosystem —
public platforms, public data, anybody may look — has no identity provider to
point at and no reason to acquire one, and Hlin is currently unusable there.

This is not only an evaluation problem. It is a category of deployment the
design is supposed to serve: a dozen platforms register against a shell, and
nothing about that premise requires knowing who is looking.

The constraint that shapes the answer is in `surfaces.rs`:

```rust
let key = format!("{surface_id}:{}", principal.sub);
```

A surface is one viewer's view of a layout — its time range, its filter
selections, its per-panel state. Keyed by `principal.sub`. So "everyone is the
same anonymous principal" is not a small simplification: every visitor shares
one surface, and one person clicking a filter moves the charts for everybody
else looking at the same page. That is the same defect `dev` is refused for,
arriving by a different route.

## Decision

A fourth strategy, `anonymous`, usable in a release build.

**Every visitor gets their own principal, from a cookie the shell sets.** On a
request with no anonymous cookie, the shell mints an unguessable value, sets it,
and uses it as `principal.sub`. No round trip, no provider, no session table —
the cookie *is* the identity, and it needs no server-side record because it
names nothing that is stored.

**A shell using `anonymous` refuses every write.** `POST /api/layouts`, `PUT`,
`DELETE` and `fork` answer 403 with a reason. The browser is told through
`/api/config` so the front end does not offer Edit at all, rather than offering
it and failing at the end.

Read-only is not a separate setting. `anonymous` plus writes is exactly `dev` —
an unauthenticated caller creating, editing and deleting whatever they like —
and `dev` is refused in a release build for that reason. Making read-only
optional here would re-introduce, behind a second flag, the thing the first
refusal exists to prevent.

Interaction is not a write. Time ranges and filter selections live in the
in-memory surface, keyed per visitor, so an anonymous viewer can drive every
control a panel draws. What they cannot do is change what anybody else sees, or
leave anything behind.

## Alternatives Analysis

| Option | Pros | Cons | Risk | Cost |
|--------|------|------|------|------|
| One shared anonymous principal | Nothing to mint, no cookie | Every visitor shares one surface: one click moves everyone's charts. The `dev` defect, renamed | High | XS |
| Allow `dev` in release builds | No new code at all | Restores exactly what its refusal exists to prevent, and lets anonymous callers write | High | XS |
| `anonymous`, per-visitor cookie, read-only | No provider, no proxy, no shared state; writes impossible so there is nothing to vandalise or impersonate | Cannot compose on an anonymous instance; layouts must come from elsewhere | Low | S |
| A separate `read_only` flag, orthogonal to strategy | Also serves "oidc, but nobody may edit" | `anonymous` + writable is `dev`; a flag makes that configurable again | Medium | S |

## Rationale

The per-visitor cookie is what makes anonymity work rather than merely compile.
Without it the feature ships a visibly broken surface, and the brokenness only
appears with a second viewer — which is to say, in production and not in a test.

Read-only is what makes it safe without an identity provider. There is no
ownership to steal because nothing is owned; forging another visitor's cookie
gains an attacker the ability to share a stranger's time range. The threat model
collapses to nothing precisely because writes are gone, which is why the two
halves are one decision and not two settings.

Composition is given up deliberately. An operator composes with a real
authenticator against the same database — [[HLIN-A-0007]]'s published layouts
are exactly the shape an open instance serves — and the open instance shows the
result. Allowing anonymous authoring would mean any visitor could delete the
surfaces every other visitor came to see.

## Consequences

### Positive
- Hlin runs, as a release build, with no identity provider and no proxy.
- The open-ecosystem deployment the vision describes becomes reachable.
- Every visitor drives their own controls; nobody moves anybody else's charts.
- The front end stops offering an action that cannot succeed.

### Negative
- Nothing can be composed on an anonymous instance. Layouts must be authored
  elsewhere, against the same database, by a shell configured with a real
  authenticator.
- A visitor who clears cookies is a new visitor, and their time range resets.
- One more strategy in a vocabulary this project keeps deliberately closed.

### Neutral
- The cookie is not a session: no database row, no expiry sweep, nothing to
  revoke. It is a name a browser keeps.
- `hlin-token` is still minted for platforms under `anonymous`; a platform that
  wants no credential ignores a header it does not read.

## Review Triggers

- Somebody needs an open instance where visitors *can* compose. That is a
  different decision — it needs ownership without identity, which this one does
  not attempt.
- A read-only mode is wanted under `oidc` or `trusted-header`. That is
  authorisation, and belongs in its own decision rather than by widening this
  one.
