---
id: 001-identity-credentials-are-pluggable
level: adr
title: "Identity credentials are pluggable strategies; forward-session is the day-one default where a shared session exists"
number: 1
short_code: "HLIN-A-0008"
created_at: 2026-09-07T15:22:06.560773+00:00
updated_at: 2026-09-07T15:25:07.947417+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-8: Identity credentials are pluggable strategies; forward-session is the day-one default where a shared session exists

## Context

[[HLIN-A-0004]] hoisted authentication to the shell and had the shell forward identity to platforms on every request. [[HLIN-S-0004]] then specified one credential for that: an Ed25519 token in `X-Hlin-Identity`, which every platform verifies with a shared crate. That is the right long-term contract, and it has a day-one cost: every platform must integrate the verification crate before Hlin can show a single panel from it.

Two facts about the platforms change the calculation. The vision says single-origin sessions sit underneath Hlin, so some or all platforms may already share a session the shell could simply forward. And a dozen platforms built over time will have a dozen existing arrangements: IdP tokens they already validate, a service mesh that already proves callers, an API key and nothing else. A shell that accepts only its own token asks every one of them to change before it can be useful to any of them.

There are two boundaries. How a person gets a principal at the shell, and what credential the shell attaches when it calls a platform. Both need more than one answer.

## Decision

**The credential is a strategy, selected per platform in shell configuration.** The shell holds one small trait for each boundary. An *authenticator* turns an incoming request into a `Principal`. A *credentialer* turns a `Principal` and a target platform into the headers a request carries. Everything downstream sees only `Principal` and never learns where it came from. The strategy set, its configuration and its security rules are in [[HLIN-S-0005]].

**Shell to platform**, in the order a deployment should reach for them:

- `forward-session`: the viewer's own session cookies, forwarded on the aggregator's request. **The day-one default wherever the platform shares the shell's session**, because it needs no change on the platform at all. Refused for any platform on a different origin.
- `hlin-token`: the credential of [[HLIN-S-0004]]. The long-term default, the only strategy usable across a trust boundary, and the one platforms migrate to.
- `forward-bearer` and `token-exchange`: the viewer's IdP token, forwarded as-is or exchanged for a platform-scoped one. No platform change where they already validate the IdP.
- `trusted-header`: plain principal headers, trusted because a mesh proves the caller.
- `static-bearer`: a per-platform key. Allowed, and it must be acknowledged explicitly in configuration, because it collapses every viewer into one principal and silently bypasses whatever per-user rules the platform has.

**Person to shell**: `trusted-header` from an auth proxy, `oidc` natively, and `dev` for development. SAML, Kerberos and client certificates are served by putting a proxy in front rather than by teaching the shell each protocol.

[[HLIN-A-0004]]'s decisions stand unchanged: authentication is hoisted, platforms authorize at fetch time, dedup keys on principal. [[HLIN-S-0004]] stands unchanged as the definition of `hlin-token`.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Pluggable strategies, `forward-session` first (chosen) | Zero platform change on day one where the shared session exists; every existing arrangement has a slot; the token remains the destination | Two traits and a config block; more surface to document and test | Low | Medium |
| `hlin-token` only, as specified | One mechanism, one crate, one thing to get right | Every platform must integrate before any panel shows; the shared session the vision assumes goes unused | Medium | Low |
| Forward the session only | Simplest possible; no token code at all | Fails the moment a platform is on another origin or trust domain; no path to a proper contract | High | Low |

## Rationale

The token is the correct contract and the wrong first step. A contract every platform must adopt before the product works is a coordination point, and coordination points are what the vision exists to remove. Making the credential a strategy means the first platform onboards with no change at all, the second onboards with whatever it already has, and the token arrives as a migration each team makes on its own schedule. That is the same shape as everything else in Hlin: the shell meets platforms where they are and never rebuilds anyone.

The two-trait shape is what keeps this from spreading. Dedup, the state machine and the stream see a `Principal` and nothing else, so adding a strategy touches one file and no consumer.

## Consequences

### Positive
- The first platform can be on a surface the day the shell runs, with no change on its side.
- Every existing authentication arrangement in the organisation has a named slot, so onboarding is a configuration question rather than an integration project.
- The token becomes a migration target with a reason to reach it, rather than a gate.

### Negative
- `forward-session` moves a viewer's cookie server-side to a platform. That is ordinary within one trust domain and a serious mistake across one, so the strategy must refuse any base that is not same-origin with the shell, and the specification says so.
- `static-bearer` makes `forbidden` unreachable and dedup global. Both are correct consequences of one-principal-for-all, and both are easy to forget six months later, which is why configuration must acknowledge them.
- More strategies means more to test. Each gets a conformance test against the credentialer trait, the same way the store has one suite for two backends.

### Neutral
- The demo implements `dev`, `hlin-token`, `forward-session` and `static-bearer`; the rest are specified and land as later tasks.
- The sample platform gains a mode where it accepts a shared demo session, so the demo shows one platform that needed no change beside one that verifies the token.
