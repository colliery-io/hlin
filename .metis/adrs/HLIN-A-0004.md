---
id: 001-authentication-is-hoisted-to-hlin
level: adr
title: "Authentication is hoisted to Hlin; identity is forwarded and dedup is per principal"
number: 1
short_code: "HLIN-A-0004"
created_at: 2026-09-07T10:59:56.254830+00:00
updated_at: 2026-09-07T11:02:27.435428+00:00
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

# ADR-4: Authentication is hoisted to Hlin; identity is forwarded and dedup is per principal

## Context

The vision promises that the aggregator deduplicates identical data requests across panels, and leaves authorization as an open question: is panel-level visibility the platform's decision, the shell's, or negotiated? The design review surfaced that these two conflict. If a platform may return different data to different users for the same request, then deduplicating across users is unsafe, and the aggregator's central promise depends on how authorization is decided.

A second fact constrains the answer: the shell polls manifests as itself, on a schedule, with no user in the loop. A manifest therefore cannot vary per user, so "the platform decides visibility via the manifest" was never available.

## Decision

Authentication is hoisted to Hlin. A user logs in once, at the shell, on the single-origin session the vision's routing layer already provides. Platforms do not run their own login for shell traffic.

On every data request the aggregator makes on a user's behalf, it forwards that user's identity to the platform as a signed token in a request header. The token is minted or re-signed by the shell, is verifiable by platforms without calling back to the shell, and carries the principal and whatever claims the identity provider supplies.

Platforms authorize at data-fetch time, using the forwarded identity. A platform that will not serve a panel to a principal returns 403; the shell treats that as a panel state, not an error. Authorization is the platform's decision; the shell never interprets platform policy and never filters the manifest.

Deduplication is keyed on `(platform, data endpoint, canonical params, principal)`. Requests from different principals never share a response, even when byte-identical, because the shell cannot know whether the platform would have answered them the same way.

### Refinement (2026-09-08, from the architectural review)

**The scope of collapsing is one surface, not one principal.** This decision originally said identical requests "collapse across every panel and every open surface that principal has". The shell collapses across every panel *on a surface* — which covers two panels reading the same endpoint, and covers a layout opened in two tabs, since one principal opening one layout gets one surface. Two *different* layouts showing the same panel with the same parameters produce two upstream requests.

The security half of this decision is unchanged and is the half that matters: the principal is part of the key, a surface exists per `(layout, principal)`, and nothing is ever shared across principals. What is narrower than written is the efficiency claim.

Amended rather than implemented, deliberately. A per-principal in-flight table would be real machinery — shared mutable state across surfaces, with its own lifetime and eviction — bought for a case that only arises when one person routinely keeps several different layouts open at once. If that becomes common, this is the paragraph to revisit; until then the ADR should describe the system that exists.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Hoist auth to shell, forward identity, dedup per principal (chosen) | One login; platforms keep full authority over their data; dedup safe by construction | Dedup gains are per user, not global; platforms must verify a shell-issued token | Low | Medium |
| Per-platform sessions, shell proxies cookies | No token plumbing | Twelve logins, or cookie forwarding across origins; the vision's single-origin session already rejects this | High | High |
| Shell decides visibility, platforms serve anyone the shell asks for | Global dedup possible | The shell must encode every platform's policy; a central bottleneck on exactly the axis the vision forbids; platforms lose authority over their own data | High | High |
| Panel data is user-independent; authz is per-platform, all-or-nothing | Global dedup; simplest | Forecloses any per-tenant or per-role panel; not credible across a dozen platforms | Medium | Low |

## Rationale

Login is the model. A user authenticates once and the result is reused everywhere, but the *action* being reused is authentication, not authorization. Forwarding identity keeps that shape: the shell does the one-time thing, and every downstream decision that depends on who is asking is made by the party that owns the data.

Keying dedup on principal makes it safe without the shell having to reason about platform policy. The cost is that dedup collapses fan-out within a user's session rather than across the whole population, but within-session is where the fan-out actually is: one person's surface with eight panels from six platforms and one time-range picker.

## Consequences

### Positive
- Authorization is resolved: platforms decide, at fetch time, with a forwarded identity. The open question in the vision closes without giving the shell any policy knowledge.
- Dedup is correct by construction; no per-platform declaration of "this data is user-independent" is needed.
- The manifest stays a single, public-within-the-shell document, so the registry, the panel picker, and layouts see one truth per platform.

### Negative
- The panel state machine needs a fifth unavailability cause, `forbidden`, for a 403 on the data endpoint. The vision names four; this extends the list and must be reflected in the stream-protocol specification.
- Every platform must verify the shell's token. That is a small shared library obligation, and a key-distribution question for the deployment design.
- A panel that a principal cannot fetch still appears in the picker, since the manifest is not filtered. Users will be able to add panels that render as forbidden for them. Acceptable for v1; a picker-side hint can come later.

### Neutral
- Dedup is per principal, so aggregate load on platforms scales with users, not with panels per user. Rate coalescing on time-range changes remains a separate aggregator concern.
