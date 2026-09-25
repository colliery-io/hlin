---
id: identity-forwarding
level: specification
title: "Identity Forwarding"
short_code: "HLIN-S-0004"
created_at: 2026-09-07T13:12:50.135107+00:00
updated_at: 2026-09-07T13:12:50.135107+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Identity Forwarding

## Overview

A person logs in once, at Hlin. Every request the shell then makes on their behalf carries who they are, and the platform decides what to answer (decision [[HLIN-A-0004]]).

This is a contract every platform implements, so it needs the same precision as the manifest. A platform that gets it wrong either refuses everyone or, worse, trusts a caller it should not. The whole of the contract is: one header, one signed token, one public key set to verify it against, and two status codes with agreed meanings.

The shell never interprets a platform's policy. It says who is asking; the platform decides. That is what keeps authorisation out of the shell without the shell having to know anything about anyone's rules.

## System Context

### Actors
- **Identity provider**: authenticates the person. Outside this specification; the shell integrates with whatever the deployment already runs.
- **Shell**: holds the session, mints a token per outgoing request, publishes its public keys, rotates them.
- **Platform**: verifies the token, decides whether this principal may have this data, answers or refuses.

### Boundaries
Inside: the token, the header, key publication and rotation, the status codes, and how a platform develops without a shell. Outside: how the shell authenticates people in the first place, what any platform's authorisation rules are, and the shell's own session cookie.

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | Every request the shell makes on a person's behalf carries a signed identity token in the `X-Hlin-Identity` header | Platforms need to know who is asking; a dedicated header leaves their own `Authorization` semantics untouched |
| REQ-1.2 | Tokens are signed asymmetrically. The shell publishes its public keys at a well-known path; a platform verifies without calling the shell | Verification must not add a synchronous dependency on the shell, and no platform should ever hold a key that mints tokens |
| REQ-1.3 | A token is minted per request, not per stream or per session | A long-lived browser stream will outlive any sane token lifetime; minting per request is the only way both can be short |
| REQ-1.4 | `aud` names the platform the request is going to. A platform rejects a token addressed to another | A token captured from one platform must not be replayable against a second |
| REQ-2.1 | A platform returns 401 when verification fails and 403 when the principal is understood but refused | The shell maps these to different panel states; conflating them makes a broken token look like a permissions problem forever |
| REQ-2.2 | The `options` endpoint of a `select` parameter receives the same token as a data endpoint | Options may legitimately differ per principal ([[HLIN-S-0002]]) |
| REQ-3.1 | Manifest fetches carry no identity token; they are made by the shell as itself | A manifest is identical for every user, so a per-user token would imply otherwise ([[HLIN-S-0001]]) |
| REQ-3.2 | A platform must be able to run locally without a shell, and the mechanism must be impossible to enable in production | Every platform team develops daily; a bypass that can escape is worse than no bypass |
| REQ-4.1 | A write (any method other than `GET` or `HEAD`) carries a token bound to its request: `htm` and `htu` name its method and path, and it lives 30 seconds. A platform refuses a write whose token is unbound or bound to another request with 401 | A read token captured in a log must not be replayable as a write ([[HLIN-A-0013]] decision 4) |
| REQ-4.2 | Reads carry unbound tokens, verified exactly as before | Binding a read buys little, since the principal may read either way, and changing reads would break every platform at once |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | Verification requires one dependency and about twenty lines | Twelve teams implement this; anything harder gets done badly or skipped |
| NFR-1.2 | Key rotation never requires a coordinated deploy | Platforms release independently; a rotation that needs everyone to ship at once is a rotation nobody performs |

## The token

A JWT, signed `EdDSA` over Ed25519.

Chosen over RSA and ECDSA for short keys, small signatures and one way to use it. `HS256` is not an option: a shared secret would put a token-minting key on every platform, which turns twelve services into twelve places the shell's identity can be forged.

### Header

```json
{ "alg": "EdDSA", "typ": "JWT", "kid": "2026-09-a" }
```

`kid` is required. It is what makes rotation work without coordination: a platform that does not recognise a `kid` refetches the key set once, rather than failing.

### Claims

| Claim | Type | Required | Meaning |
|---|---|---|---|
| `iss` | string | yes | The shell's identifier, from platform configuration |
| `sub` | string | yes | The principal. Stable, opaque, and the same string the shell keys deduplication on |
| `aud` | string | yes | The target `platform.id`, exactly as shell configuration names it |
| `exp` | integer | yes | Expiry. 120 seconds after `iat` |
| `iat` | integer | yes | Issued at |
| `jti` | string | yes | Unique per token |
| `name` | string | no | Display name, for a platform that wants to show who is asking |
| `email` | string | no | Where the identity provider supplies one |
| `groups` | array of strings | no | Whatever the identity provider asserts, passed through unchanged |
| `htm` | string | on a write | The request's method, exactly as sent. Present only on a token bound to one request |
| `htu` | string | on a write | The request's path relative to the platform's base, without its query, in the normal form below. Present only with `htm` |

`groups` is passed through, not interpreted. The shell does not know what any group means and never decides anything from one; a platform that uses groups is using its identity provider's data, with the shell as courier.

A worked token payload:

```json
{
  "iss": "hlin",
  "sub": "u_01H8XK2P",
  "aud": "orebank",
  "iat": 1757244600,
  "exp": 1757244720,
  "jti": "01H8XK2PQR7V",
  "name": "A Person",
  "groups": ["platform-engineering", "oncall"]
}
```

### Lifetime

120 seconds for a read, 30 seconds for a bound write, because a token is minted per request and needs only to survive the request. A bound token is sent the moment it is minted, so anything longer is only more time for the same write to be replayed. `jti` is present so a platform *may* keep a replay cache, but with a two-minute window over TLS on an internal network, most will not need one. It costs nothing to include and cannot be added later without a version bump.

### Binding a write to its request

A write's token names the one request it was minted for, so a token captured from a log cannot be spent on a different write, and a read token cannot be spent on any. The names follow DPoP (RFC 9449), without its proof key: the shell is the only party that sees the request before it is sent, so there is no one else to hold a key.

`htu` is the path relative to the platform's base, not a full URL, because the shell addresses a platform by its configured base and the platform may not know the host name the shell used. Both sides reduce the path to one normal form before comparing, and the rules exist because a comparison that normalises differently on the two sides fails silently:

1. Drop everything from the first `?` or `#`.
2. The path must start with `/`. `/` on its own is the root.
3. Split on `/`. Every segment must be non-empty, so a double slash or a trailing slash is refused, not collapsed.
4. Percent-decode each segment exactly once. `%41` is `A`; `%2541` is `%41`, a different path.
5. Refuse a malformed escape, bytes that are not UTF-8 once decoded, a literal backslash, a segment that decodes to `.` or `..`, and a segment that decodes to contain `/`, `\` or a control character.
6. Compare case-sensitively, character for character.

Refused shapes are refused rather than normalised because resolving `..` or collapsing slashes would make the binding agree with one server's reading of a path and not another's. The shell's request proxy refuses the same shapes before minting ([[HLIN-S-0007]]), so a request sent through the shell never meets them.

`htm` is compared exactly. Methods are case-sensitive, so `post` is not `POST`, and only `GET` and `HEAD` exactly are reads; any other method, including one nobody has heard of, is a write.

A worked write payload:

```json
{
  "iss": "hlin",
  "sub": "u_01H8XK2P",
  "aud": "orebank",
  "iat": 1757244600,
  "exp": 1757244630,
  "jti": "01H8XK2PQR7W",
  "htm": "POST",
  "htu": "/api/batches/42/retry"
}
```

## Keys

The shell publishes a JWKS document:

```
GET <shell>/.well-known/hlin-keys.json     application/json
```

```json
{ "keys": [
  { "kty": "OKP", "crv": "Ed25519", "kid": "2026-09-a", "x": "...", "use": "sig", "alg": "EdDSA" },
  { "kty": "OKP", "crv": "Ed25519", "kid": "2026-08-a", "x": "...", "use": "sig", "alg": "EdDSA" }
] }
```

Unauthenticated: public keys are public, and requiring a credential to fetch them would create the bootstrapping problem this design exists to avoid.

### Rotation

| Step | What happens |
|---|---|
| Publish | The new key appears in the JWKS alongside the old one. Nothing signs with it yet |
| Wait | One propagation window, default 24 hours, so every platform's cache has seen it |
| Switch | The shell starts signing with the new `kid`. The old key stays published |
| Retire | After a second window, the old key leaves the JWKS |

A platform caches the key set for up to an hour, and refetches immediately on an unrecognised `kid`, rate-limited to once a minute. Those two rules together are what make rotation need no coordinated deploy (NFR-1.2): the publish-then-switch order means a platform always has the key before it sees a token signed with it, and the `kid` miss is a safety net for a platform whose cache is cold.

An emergency rotation skips the waiting and accepts that platforms will take a `kid` miss.

## What a platform must do

On every request to a panel data endpoint or a `select` options endpoint:

1. Read `X-Hlin-Identity`. Absent, in production, is a 401.
2. Verify the signature against the key with the matching `kid`, refetching the key set once if unrecognised.
3. Check `iss` matches the configured shell, `aud` matches this platform's own id, and `exp` has not passed. Allow 60 seconds of clock skew on `exp` and `iat`.
4. On a write, require `htm` and `htu`, `htm` equal to the request's method, `htu` equal to the request's path in the normal form above, and `exp - iat` no more than 30 seconds. On a read, accept an unbound token as before. A token that carries `htm` or `htu` is held to them whatever the method, and one carrying only one of them, or a non-string, is refused. Any failure is a 401.
5. Take `sub` as the principal. Apply the platform's own rules.
6. Answer, or refuse with 403.

### Status codes

| Code | Meaning | Panel state |
|---|---|---|
| 200 | Here is the data | `ready`, once the envelope validates |
| 401 | The token was absent, unverifiable, expired, for another audience, or on a write not bound to this request | `unavailable (malformed)` |
| 403 | The token was fine; this principal may not have this | `unavailable (forbidden)` |

The distinction is load-bearing. A 401 is the shell's fault or a misconfiguration, and an operator needs to see it. A 403 is the system working correctly, and the viewer needs to see it. A platform that returns 403 for a bad token makes a broken deployment look like a permissions problem forever, and nobody investigates permissions.

## Development without a shell

A platform team runs their service every day with no shell in front of it. Two mechanisms, and the safety of both rests on the same principle: the bypass is a property of the *build*, not of the configuration.

**Development keys.** A platform points at a local shell, or at a small key server the shell crate ships, and gets real tokens. This is the recommended path: the code under test is the production code path.

**The bypass.** A build compiled with the `hlin-dev-identity` feature accepts a request with no token and treats it as a fixed development principal. The feature is refused at compile time in release builds, and the verification library carries a test asserting exactly that. Configuration cannot enable it; an environment variable cannot enable it; nothing at runtime can enable it. A shipped binary that would accept an unsigned request does not build (REQ-3.2).

That is stricter than a flag with a scary name, deliberately. A flag that can be set is a flag that will be set, at three in the morning, to make something work.

## The verification library

Twelve platforms implementing this from prose is twelve chances to get it wrong, and the failure that matters is the quiet one: a platform that verifies the signature but forgets `aud`, and so accepts tokens minted for someone else.

A small crate should ship this: fetch and cache the key set, verify, check the claims, expose the principal, and return the right status code for each failure. It is a task for a later initiative, and this specification is written so that a platform implementing it by hand is doing something reasonable rather than something reckless.

`hlin-identity` is that crate. `Verifier::verify` checks a read; `Verifier::verify_request` takes the method and path as well and applies the write rule; the `axum` feature's `HlinRequestIdentity` extractor does the same from a request and answers a refusal with a 401 whose body says why. `Issuer::mint_bound` mints a bound token from a `BoundRequest`, which normalises the path and refuses one that cannot be bound before anything is signed.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0004]] | Auth hoisted, identity forwarded, dedup per principal | decided | This specification is that decision's contract |
| [[HLIN-A-0002]] | Content hash + semver, enforced at runtime | decided | Manifest fetches carry no identity, so contract checking is per platform not per person |
| [[HLIN-A-0013]] | Requests carried with identity bound to each | decided | Decision 4: a write's token is bound to its method and path with a 30-second lifetime; reads stay unbound |

## Open Items

- Whether `sub` should be the identity provider's subject or a shell-issued pseudonym. A pseudonym keeps the provider's identifiers out of twelve platforms' logs; the provider's own subject is what those platforms already key on elsewhere. The pseudonym is probably right and costs a mapping table.
- Whether the shell should sign the request path as well as the audience, so a token cannot be replayed against a different endpoint of the same platform. **Closed for writes** by [[HLIN-A-0013]]: a write's token carries `htm` and `htu` (see *Binding a write to its request*). **Still open for reads**, where it buys little, since the principal may read either way.
- Service-to-service calls: a platform calling another platform on a person's behalf is out of scope here and will want its own answer.
- Whether a platform should be able to declare in its manifest that it requires identity, so the shell can tell an operator about a platform that is not checking.
