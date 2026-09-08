---
id: identity-strategies
level: specification
title: "Identity Strategies"
short_code: "HLIN-S-0005"
created_at: 2026-09-07T15:22:07.282328+00:00
updated_at: 2026-09-07T15:22:07.282328+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Identity Strategies

## Overview

The shell attaches an identity to every request it makes on a person's behalf, and platforms decide what to answer ([[HLIN-A-0004]]). This specification defines the *strategies* by which that happens, per [[HLIN-A-0008]]: how a person becomes a `Principal` at the shell, and what credential the shell attaches when it calls each platform.

[[HLIN-S-0004]] is unchanged and defines one strategy here, `hlin-token`, in full. This document is the catalogue around it: the shape every strategy shares, the configuration that selects one, and the rules that keep each one safe.

## System Context

### Actors
- **Auth proxy or identity provider**: whatever authenticates people in the deployment. Outside this specification; the shell integrates through an authenticator strategy.
- **Shell**: runs one authenticator and, per platform, one credentialer.
- **Platforms**: receive whatever credential their configured strategy sends, and authorize.

### Boundaries
Inside: the `Principal`, the two traits, every strategy's behaviour and configuration, and the security rules per strategy. Outside: the token format (in [[HLIN-S-0004]]), any platform's authorization rules, and the shell's own session cookie mechanics.

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | Every part of the shell downstream of authentication sees a `Principal` and nothing about how it was obtained | Adding a strategy touches one file and no consumer |
| REQ-1.2 | The credential strategy is selected per platform in configuration; a deployment may mix strategies freely | Platforms are onboarded where they are, one at a time |
| REQ-1.3 | `forward-session` refuses any platform base that is not same-origin with the shell, at configuration load, before any request is made | A viewer's cookie must never cross a trust boundary |
| REQ-1.4 | `static-bearer` requires an explicit `acknowledge_shared_principal = true` in its configuration block, and the shell logs a warning at startup for each platform using it | One principal for all bypasses per-user authorization; that must be a decision, not a default |
| REQ-1.5 | Every credentialer produces headers only; it never rewrites the request body or path | Strategies are interchangeable from the aggregator's point of view |
| REQ-2.1 | Dedup keys on `Principal.sub` regardless of strategy; where a strategy collapses viewers to one principal, dedup is correspondingly global | The dedup key must be exactly as fine as the identity the platform sees ([[HLIN-A-0004]]) |
| REQ-2.2 | Manifest fetches use no credentialer; they are made by the shell as itself | A manifest is identical for every user ([[HLIN-S-0001]]) |
| REQ-3.1 | Every strategy passes the same conformance suite against its trait | A strategy that behaves differently from the others is a bug, not a feature |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | A strategy is one file implementing one trait; no strategy is referenced by name anywhere except configuration parsing and its own tests | Keeps the seam honest |
| NFR-1.2 | Misconfiguration fails at startup with a message naming the platform and the rule, never at first request | An operator finds out when they deploy, not when a viewer does |

## The Principal

What every strategy produces and every consumer reads.

| Field | Type | Meaning |
|---|---|---|
| `sub` | string | Stable, opaque identifier. The dedup key and the `sub` of any minted token |
| `name` | string, optional | Display name |
| `email` | string, optional | Where the source supplies one |
| `groups` | array of strings | Whatever the source asserts, passed through and never interpreted by the shell |
| `credentials` | opaque | What the authenticator captured that a credentialer may need later: the session cookies, the IdP access token. Never logged, never serialised, never stored |

`credentials` is what makes forwarding strategies possible without the authenticator knowing which credentialer will run. It is held in memory for the life of the stream and dropped with it.

## The two traits

```rust
trait Authenticator {
    /// Turn an incoming request into a principal, or refuse it.
    fn authenticate(&self, request: &Request) -> Result<Principal, Refusal>;
}

trait Credentialer {
    /// The headers a request to `platform` must carry for `principal`.
    fn headers(&self, principal: &Principal, platform: &PlatformConfig) -> Result<HeaderMap, CredentialError>;
}
```

The aggregator calls `headers` once per upstream request and attaches the result. Nothing else in the shell calls either trait.

## Authenticator strategies: how a person becomes a principal

Exactly one is configured per deployment.

### `dev`

A fixed principal from configuration. Refused in release builds by the same compile-time rule as the platform-side bypass in [[HLIN-S-0004]].

```toml
[auth]
strategy = "dev"
principal = { sub = "u_dev", name = "Development User", groups = ["platform-engineering"] }
```

### `trusted-header`

Identity from a proxy in front of the shell: oauth2-proxy, Pomerium, Cloudflare Access, Google IAP, Teleport. The most common day-one shape in an organisation that already has a proxy.

```toml
[auth]
strategy = "trusted-header"
subject_header = "X-Forwarded-User"
email_header = "X-Forwarded-Email"
groups_header = "X-Forwarded-Groups"
trusted_sources = ["10.0.0.0/8"]        # required
signed_header = { name = "Cf-Access-Jwt-Assertion", jwks = "https://…/certs" }   # optional, preferred where the proxy offers one
```

Rules: `trusted_sources` is required and a request from outside it with the headers present is refused and logged. Where the proxy signs an assertion, the signature is verified and the plain headers are ignored. Without a signed assertion, the proxy is the security boundary and the shell must not be reachable except through it; the configuration documentation says so in those words.

### `oidc`

Authorization code with PKCE against any OpenID Connect provider. The shell issues its own session cookie and holds the IdP tokens in `Principal.credentials` for the forwarding strategies.

As built (2026-09-08), a session keeps the subject, name and groups the id token asserted, and **not** the provider's access or refresh tokens. The two credentialers that would need them, `forward-bearer` and `token-exchange`, are not built either, so keeping tokens would mean holding a credential for every signed-in person against a use nothing makes yet. Whichever of those is built first adds the column and the refresh that keeping them honestly requires.

```toml
[auth]
strategy = "oidc"
issuer = "https://login.example.com/"
client_id = "hlin"
client_secret_env = "HLIN_OIDC_SECRET"
public_url = "https://hlin.example.com"       # required; the redirect URI is built from it
scopes = ["openid", "profile", "email", "groups"]
groups_claim = "groups"
session_hours = 12

[auth.cookie]
name = "hlin_session"
secure = true                                  # refused where public_url is https and this is false
same_site = "Lax"                              # Strict is refused; see below
```

Rules, all checked at startup rather than at the first sign-in:

- The issuer must be `https`. It is where the shell fetches the keys it will trust to say who somebody is, and over plain HTTP anything on the path chooses those keys.
- `public_url` is required and cannot be derived. The redirect URI must match what was registered with the provider exactly, and a shell behind a proxy cannot see its own public address from the inside.
- `scopes` must include `openid`, or there is no id token and nothing to learn an identity from.
- A shell served over `https` whose cookie is not `Secure` is refused.
- `SameSite=Strict` is refused. The provider returns the browser by a top-level redirect, which `Strict` treats as cross-site and strips the cookie from, so a person would arrive back at the shell signed in and apparently not, forever.

**Sessions are rows, not signed cookies** (decided 2026-09-08, under [[HLIN-T-0026]]). The alternative — a self-contained cookie carrying the claims — cannot be withdrawn: signing out clears the browser's copy and nothing else, and there is no answer to "revoke this person now" short of rotating a key and ending everyone's session at once. It would also put the provider's tokens in the browser, on a cookie that then goes to every same-origin platform on every request. What a row costs is one indexed read per request and a table that grows, and a sweeper answers the second. A shell restart signs nobody out, which is the behaviour to want: the alternative logs out an organisation because a shell was redeployed.

The cookie's value is never stored. A session is keyed by its SHA-256, so a dump of the table is a set of useless hashes rather than a set of live sessions; the shell has the value on every request and can hash it, so storing it buys nothing.

A refused request carries where to sign in, where there is anywhere: `401` with `{"login": "/auth/login"}` under `oidc` and `{"login": null}` otherwise. Two shells answer the same status to the same request and a browser must do opposite things about it — go and sign in, or say that something upstream is broken — and the status cannot carry that distinction.

### Not strategies

SAML, Kerberos and client certificates are handled by a proxy that speaks them, feeding `trusted-header`. Teaching the shell each protocol buys nothing a proxy does not already provide.

## Credentialer strategies: what the shell sends a platform

One per platform, in that platform's configuration block.

### `forward-session` — the day-one default where it applies

Forwards the viewer's own session cookies to the platform. The platform sees exactly the request the browser would have made, and needs no change.

```toml
[[platforms]]
id = "orebank"
base_url = "https://apps.example.com/orebank"
auth = { strategy = "forward-session", cookies = ["session"] }
```

Rules: the platform base must be same-origin with the shell, checked at startup (REQ-1.3). Only the named cookies are forwarded. The cookies are read from `Principal.credentials`, captured when the stream was opened, and are never persisted. If the shell itself uses `oidc`, its own session cookie is never forwarded.

Why the same-origin rule is absolute: a cookie is a bearer credential for everything on its origin. Sending it anywhere else hands that everything to whoever answers.

### `hlin-token` — the long-term default

The credential of [[HLIN-S-0004]], minted per request by the shell's issuer. The only strategy usable across a trust boundary, and the one platforms migrate to.

```toml
auth = { strategy = "hlin-token" }
```

### `forward-bearer`

Forwards the viewer's IdP access token as `Authorization: Bearer`. Requires the `oidc` authenticator, since that is where the token comes from. No platform change where they already validate the IdP's tokens.

```toml
auth = { strategy = "forward-bearer" }
```

Rules: the token's audience is whatever the IdP minted for the shell, so a platform that checks `aud` strictly will refuse it. That is the platform being correct, and the fix is `token-exchange`, not a looser platform.

### `token-exchange`

Exchanges the viewer's token for one scoped to the target platform via RFC 8693, at the IdP, then sends that as a bearer. The correct form of `forward-bearer`. Requires `oidc` and an IdP that supports exchange; Keycloak, Entra and Okta do. Exchanged tokens are cached per `(principal, platform)` until shortly before expiry.

```toml
auth = { strategy = "token-exchange", audience = "orebank-api" }
```

### `trusted-header`

Plain principal headers, trusted because the network proves the caller: mutual TLS, SPIFFE, a service mesh. The shell asserts; the mesh signs.

```toml
auth = { strategy = "trusted-header", subject_header = "X-Hlin-Subject", groups_header = "X-Hlin-Groups" }
```

Rules: this strategy carries no proof of its own. The configuration documentation states that it is only safe where the platform cannot be reached by anything that is not the shell, and gives the mesh as the way to arrange that.

### `static-bearer`

A per-platform key. For platforms with machine authentication and nothing else.

```toml
auth = { strategy = "static-bearer", token_env = "HLIN_OREBANK_TOKEN", acknowledge_shared_principal = true }
```

Rules: `acknowledge_shared_principal` is required (REQ-1.4). Every viewer reaches the platform as the same caller, so the platform's per-user rules cannot apply, `forbidden` cannot occur for a viewer, and dedup is global for that platform. The shell logs one warning per such platform at startup naming all three consequences.

## Strategies by scope

| Strategy | Side | In the demo | Later task |
|---|---|---|---|
| `dev` | authenticator | yes | |
| `trusted-header` | authenticator | | yes |
| `oidc` | authenticator | | yes |
| `forward-session` | credentialer | yes | |
| `hlin-token` | credentialer | yes | |
| `static-bearer` | credentialer | yes | |
| `forward-bearer` | credentialer | | yes, with `oidc` |
| `token-exchange` | credentialer | | yes, with `oidc` |
| `trusted-header` | credentialer | | yes |

The demo runs two sample platforms: one with `forward-session`, needing no change, and one with `hlin-token`, verifying. That is the migration story in one screen.

## Conformance

One suite, run against every credentialer: produces headers and nothing else; is deterministic for a fixed principal and platform; refuses a misconfiguration at construction rather than at call; never includes the principal's `credentials` in anything it logs. One suite against every authenticator: refuses a request with no identity; produces a `Principal` with `sub` set; never panics on malformed input.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0008]] | Identity credentials are pluggable strategies | decided | This specification is that decision's catalogue |
| [[HLIN-A-0004]] | Auth hoisted, identity forwarded, dedup per principal | decided | Unchanged; strategies are how "forwarded" is done |

## Open Items

- Whether `forward-session` should also forward the `Origin` and `Referer` the browser sent, for platforms with CSRF checks on read endpoints. Data endpoints are `GET`, so probably not, but a platform doing something unusual would want it configurable.
- Per-platform group mapping: a platform may want groups under different names than the IdP asserts. A `groups_map` on the credentialer is the obvious place; deferred until one asks.
- Whether the shell should expose which strategy each platform uses on `/api/platforms`, for operators. Probably yes, and probably not to viewers.
