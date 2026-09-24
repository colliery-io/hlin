---
id: a-person-signs-in-and-changes
level: initiative
title: "A person signs in and changes things on two platforms"
short_code: "HLIN-I-0010"
created_at: 2026-09-24T17:02:57.785059+00:00
updated_at: 2026-09-24T20:14:57.748949+00:00
parent: HLIN-V-0001
blocked_by: [HLIN-I-0011]
archived: false

tags:
  - "#initiative"
  - "#phase/design"


exit_criteria_met: false
estimated_complexity: L
initiative_id: a-person-signs-in-and-changes
---

# A person signs in and changes things on two platforms

## Context

Every demo so far has been one person, looking. The `dev` authenticator makes
every request the same fixed principal, and everything the shell sends a
platform is a GET. [[HLIN-I-0005]] gave components a way to talk back, but
deliberately only as far as the shell: `Intent::Select` and `Intent::Range`
change what the shell *asks*, never what a platform *holds*. Its own words:
"Nothing here gives a design system a capability the chrome does not have."

That leaves three claims the product will be asked about and cannot yet show:

- **Somebody signs in.** `oidc` and `trusted-header` exist and are tested, but
  the demo has never run either, so no one has watched a real login land a
  person on a surface.
- **Who they are reaches the platform, and the platform decides.** The
  `hlin-token` carries `sub`, `name` and `groups` to every platform already
  ([[HLIN-S-0004]]), but no sample platform does anything with them except
  check the signature. Authentication is hoisted to Hlin ([[HLIN-A-0004]]);
  authorization never was, and nothing demonstrates that.
- **A person can change something.** Add a to-do, cross it off, post to a feed,
  from one surface, against two servers that know nothing of each other.

The third is new capability, not a demo detail. It needs a write path from
browser to shell to platform, and that path is a contract decision on the order
of [[HLIN-A-0010]] and [[HLIN-A-0011]].

## Goals & Non-Goals

**Goals:**
- Two new, independent sample platforms, each with its own state and its own
  authorization rules:
  - **Checklist**: a multi-user to-do list. Add, edit, cross off.
  - **Feed**: a multi-user web feed. Post, and edit what you posted.
- A real login in the demo, through the `oidc` authenticator the shell already
  has, against a local identity provider with several named users.
- Each platform ships its own UI as a module ([[HLIN-A-0014]]), and a
  person's changes travel from that module through the shell to the platform
  with their identity bound to the request ([[HLIN-A-0013]]).
- A surface showing both side by side, where two people signed in at once see
  each other's changes arrive without reloading.
- Refusals that read as refusals: a person editing somebody else's item is told
  so by the platform, and the UI shows it.
- A walkthrough and browser tests that assert all of this, as the existing demo
  does.

**Non-Goals:**
- Central role or permission management in the shell. Hlin authenticates and
  forwards; each platform authorizes (decision 3 below).
- Replacing the existing sample platform or its demo. `orebank` and `stampmill`
  stay as they are; this is a second demo flavour beside them.
- Persistence for the new platforms beyond memory. A restart empties them, and
  the demo says so.
- The module host itself: frames, bridge, proxies and SDK are
  [[HLIN-I-0011]]. This initiative is their first real use.
- Offline or optimistic updates. A change shows when the platform confirms it.

## Decisions

| # | Decision | Choice | Why |
|---|---|---|---|
| 1 | How a change reaches a platform | **From the platform's own module, over the bridge, through the shell's request proxy**, under declared prefixes. [[HLIN-A-0013]], [[HLIN-A-0014]] | The browser never holds a credential and never calls a platform; the platform is the one owning the frame |
| 2 | Login | **Dex, through the existing `oidc` authenticator** | Real login, no new strategy |
| 3 | Authorization | **Each platform, from the token and its own data.** Ownership plus claims or attributes. The shell holds no roles | Composed platforms each have their own idea of what a user may do |
| 4 | Drawing | **Each platform's own Leptos module**, built with the SDK and the shared kit | The demo proves the vision as amended, not the path it replaced |
| 5 | Telling the UI what a person may do | **Nothing crosses to the shell.** A module may hide what its platform would refuse; the platform enforces regardless, and its module shows the refusal | Platforms receive identity and enforce; they do not describe their authz to the shell |
| 6 | Write token | **Bound to the request**: `htm` + `htu`, shorter expiry | A captured read token cannot be replayed as a write |
| 7 | Attributes | **Fixed set: `sub`, `name`, `email`, `groups`.** `oidc` sessions start storing `email` | Enough for claims-based rules; everything else a platform keeps itself |
| 8 | Dex over plain http | **Allowed for a loopback issuer in a debug build only**, the same boundary `dev` already uses. Reversible | A self-signed Dex would put a certificate warning in the demo's first screen |

## Detailed Design

### Identity

- `oidc` reads `email` into `Claims`, the session stores it (a migration), and
  the principal carries it, so it reaches the token.
- `OidcConfig::check` accepts an `http://` issuer only on a loopback host and
  only in a debug build.
- Bound tokens are [[HLIN-I-0011]]'s; this initiative uses them.

### Sample platforms

Two new crates, each a reference a platform team can copy. Each has a server
(state in memory, `hlin-identity` verification on every request, bound tokens
required on writes, recent idempotency keys remembered, an event stream that
announces every change) and a Leptos module built with the SDK and the kit.

**`hlin-sample-checklist`**: lists, each with an owner and members, kept by the
platform itself. The module shows the chosen list's items with a checkbox to
cross off, inline edit, delete, and an add field, and a list picker drawn from
the lists the viewer belongs to. Rules: members read, add and toggle; the
item's author or the list's owner edit and delete; anyone else reading a list
gets 403, which the module shows as the platform worded it.

**`hlin-sample-feed`**: posts, newest first, with a compose box, and edit and
delete on each post. Rules, from claims: anyone signed in reads; only
`@example.com` addresses may post; authors edit and delete their own. A
platform-local setting (a muted list in its own config) shows a per-user rule
that lives nowhere but the platform.

Both manifests also declare a shell-drawn fallback (`records.v1` as `table`),
so each panel still shows something if its module cannot load.

### Demo

- Dex in `docker-compose.yml`, config in `demo/dex.yaml`: `alice@example.com`,
  `bob@example.com`, `carol@elsewhere.org`, all with password `password`;
  static client `hlin`.
- `demo/hlin-collab.toml`: `oidc` against Dex; `checklist` and `feed` on
  `hlin-token`.
- An angreal flavour for `demo up` that builds both modules, starts Postgres,
  Dex, both platforms and the shell, and publishes a layout with both panels
  side by side.
- The story: Alice and Bob sign in in two browsers. Alice adds an item; it
  appears for Bob. Bob crosses it off; Alice sees it crossed. Bob tries to edit
  Alice's post and the feed refuses, in the feed's words. Carol reads the feed
  and is refused when she posts; she opens the team list and is refused.

### Testing

- Each sample platform: its authorization rules, table-driven, with a token
  per user, including unbound and mismatched write tokens.
- `oidc`: email carried from provider to token.
- Walkthrough steps, and Playwright with two browser contexts signed in
  through Dex, driving the modules inside their frames.

## Alternatives Considered

- **Browser calls the platform directly with a token from the shell.**
  Rejected. It needs CORS on every platform, puts a bearer token in page
  script, and breaks the rule that the browser only ever talks to the shell.
- **Platforms send per-row permission hints to the shell.** Rejected in design
  (decision 5): platforms receive identity and enforce; they do not describe
  their authorization back to the shell.
- **Checklist and feed as shell-drawn `table` panels with declared actions.**
  The first design. Superseded when [[HLIN-A-0014]] made platform modules the
  primary path; kept only as each panel's fallback.
- **A dev "sign in as" picker instead of an identity provider.** Rejected. It
  would be a second pretend strategy beside `dev`, and the real `oidc` path
  would stay unexercised.
- **Keycloak.** Rejected for the demo: heavier to start and to keep configured.
  Groups were its advantage, and decision 3 leaves groups to each platform.
- **Central roles in the shell.** Rejected by decision 3.

## Implementation Plan

Proposed decomposition. Not yet created as tasks. Blocked on
[[HLIN-I-0011]] where marked.

| # | Task | Depends on |
|---|---|---|
| 1 | `oidc` email into the session and token; loopback http issuer in debug | nothing |
| 2 | Dex in compose, `demo/hlin-collab.toml`, sign-in working end to end | 1 |
| 3 | `hlin-sample-checklist` server: lists, members, rules, event stream | bound tokens from [[HLIN-I-0011]] |
| 4 | `hlin-sample-feed` server: posts, claims rules, muted list, event stream | bound tokens from [[HLIN-I-0011]] |
| 5 | Checklist module | 3, the SDK from [[HLIN-I-0011]] |
| 6 | Feed module | 4, the SDK from [[HLIN-I-0011]] |
| 7 | angreal flavour and published layout | 2, 5, 6 |
| 8 | Walkthrough and two-browser Playwright tests | everything |

Exit: the collaborative flavour of `demo up`, followed by its walkthrough and
browser tests, passes from a clean checkout, and the story in *Demo* can be
done by hand in two browsers.

## Status Updates

### 2026-09-24 — discovery opened

Discovery decisions 1–3 taken with the user. Decision 4 is deferred to design
on purpose. Nothing implemented yet.

### 2026-09-24 — design drafted

Moved to design. [[HLIN-A-0013]] drafted. Four decisions added with the user:
drawing through existing kinds plus `component` (4), no authorization hints
from platforms (5), request-bound write tokens (6), and a fixed attribute set
including email (7). Decision 8 (plain-http Dex on loopback in debug builds)
was taken during design after finding `OidcConfig::check` refuses non-https
issuers. It is reversible.

Raised and deliberately kept out of scope: a platform wanting a component that
is not in the shared library. That touches the vision's constraint that no
code crosses the boundary, and is tracked separately.

### 2026-09-24 — design paused on a vision decision

The question above did not stay separate. Asked what Hlin is for, the user
chose: the place people work (not a vantage), platforms decide how they look,
only our own platforms, and hypermedia (server-rendered, no platform code in
the page). That reverses the vision's central bet and is drafted as
[[HLIN-A-0014]], with a redline of [[HLIN-V-0001]] that has not been applied.
[[HLIN-A-0013]] is revised to match: writes are posts under declared route
prefixes, not per-action manifest declarations.

If A-0014 is decided, this design changes as follows. Nothing above is
rewritten until then.

- **Still holds:** decisions 2, 3, 5, 6, 7, 8; the identity amendment; the
  sample platforms' rules; Dex; the demo story; the testing approach.
- **Superseded:** decision 1 (per-action declarations become route prefixes),
  decision 4 (the checklist and feed render their own fragments rather than
  `table` + `component`), the manifest `actions` schema, the typed input
  vocabulary, the chrome's fallback forms, and the demo pack components.
- **New dependency:** the fragment path from A-0014 (views in the manifest,
  the sanitiser, HTML on the stream, the shell's hypermedia runtime, a first
  kit). It is larger than this initiative and probably its own, which this one
  would follow. [[HLIN-T-0059]] is answered by the same decision.

### 2026-09-24 — micro UI, sandboxed

The user set the goal as micro UI and chose a sandboxed iframe per panel.
[[HLIN-A-0014]] is rewritten around that and the hypermedia draft withdrawn.
[[HLIN-A-0013]] is revised: a module's requests travel over a `postMessage`
bridge to the shell's page, then to `/p/{platform}/…`, under declared prefixes,
with request-bound identity. The platform is the one owning the frame, never
one the module names.

For this initiative, if A-0014 is decided:

- **Still holds:** decisions 2, 3, 5, 6, 7, 8; the identity amendment; the
  sample platforms' authorization rules; Dex; the demo story; the testing
  approach.
- **Changes:** the checklist and feed each ship a Leptos module built with the
  bridge SDK, and draw their own refusals. Decisions 1 and 4, the manifest
  `actions` schema, typed inputs, chrome fallback forms and demo pack
  components all go.
- **New dependency:** the module host (frame and sandbox policy, asset proxy
  and CSP, the bridge and its protocol, the `/p/` request proxy, relaying
  `changed`, the SDK crate, the drag shield). That is its own initiative. This
  one becomes its first real use, and the demo its proof.

### 2026-09-24 — re-cut to follow the module host

[[HLIN-A-0013]] and [[HLIN-A-0014]] decided, vision amended. This initiative
now depends on [[HLIN-I-0011]] (the module host) and is its first real use.
Decisions 1, 4 and 5 rewritten for modules; the manifest `actions` schema,
typed inputs, chrome forms and demo pack components are gone. Tasks 1 and 2
(sign-in with Dex) need nothing from the module host and can start now.
