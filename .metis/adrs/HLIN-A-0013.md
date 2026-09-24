---
id: 001-platforms-may-declare-actions-the
level: adr
title: "A platform declares where it accepts requests; the shell carries them with the viewer's identity bound to each, and the platform decides"
number: 1
short_code: "HLIN-A-0013"
created_at: 2026-09-24T20:24:06.399823+00:00
updated_at: 2026-09-24T22:22:28.047658+00:00
decision_date: 2026-09-24
decision_maker: Dylan Storey
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR: A platform declares where it accepts requests; the shell carries them with the viewer's identity bound to each, and the platform decides

## Context

Everything the shell sends a platform is a GET. [[HLIN-I-0005]] gave components
a way to talk back, and kept it narrow on purpose: `Intent::Select` and
`Intent::Range` change what the shell *asks*, never what a platform *holds*.
That was right for what it was for, and it means a person can look at a to-do
list on a surface but cannot cross anything off it.

[[HLIN-I-0010]] needs the second thing: add an item, cross it off, post to a
feed, from one surface, against two platforms that know nothing of each other,
as a signed-in person whom each platform judges by its own rules.

Three constraints already in force shape the answer:

- **The browser never calls a platform.** A platform is another origin and
  expects the shell's credential, not the viewer's session ([[HLIN-A-0004]],
  [[HLIN-S-0005]]).
- **The shell never fetches an address it was handed.** The options route
  takes a platform, a panel and a control, and looks the address up in the
  manifest, precisely so it is not an open proxy for its own credential.
- **Authentication is hoisted to Hlin; authorization is not.** A platform
  receives who is asking and decides what they may do. Every composed platform
  has its own idea of that, usually per user, and a central role model would be
  exactly the coordination point the vision exists to remove.

## Decision

**A platform declares where it accepts requests, and the shell carries a
person's requests there, and nowhere else, with the viewer's identity bound to
that one request.**

1. **Declared by prefix.** A manifest declares the route prefixes under which
   the shell may carry a module's requests, reads and writes separately. The
   module ([[HLIN-A-0014]]) names the path and the platform validates what
   arrives. There is no per-action declaration and no shell vocabulary of
   input types: the module and its endpoints ship in the same platform deploy,
   so there is nothing between them for a contract to guard.

2. **Carried over the bridge, checked against the declaration.** A module's
   `fetch` reaches the shell's page as a bridge message, and the page sends it
   to `/p/{platform}/{path}` with the session cookie. The platform is the one
   that owns the frame the message came from, never one the module names. The
   shell refuses any path outside that platform's declared prefixes, any `..`,
   and anything absolute, so it cannot be pointed anywhere the platform did not
   claim.

3. **Only a person can act.** The route takes the `Author` extractor, so a
   read-only shell ([[HLIN-A-0012]]) refuses every action exactly as it
   refuses every layout write. A platform whose credential strategy collapses
   every viewer into one caller (`static-bearer`, `none`) cannot tell who is
   acting, so the shell will not send it actions and says so. Its actions are
   shown as unavailable, not offered and then refused.

4. **Identity is bound to the request.** An action carries the same
   `hlin-token` as a read, plus `htm` (the method) and `htu` (the endpoint path,
   relative to the platform's base), with a shorter lifetime. A platform
   verifying an action requires them to match, so a read token captured in a
   log cannot be replayed as a write. Reads stay unbound. This closes the open
   question in [[HLIN-S-0004]] for writes and leaves it open for reads.

5. **The platform decides, and says so in its own words.** Hlin sends no
   authorization information and receives none. The platform reads `sub`,
   `name`, `email` and `groups` from the token, combines them with whatever it
   keeps per user, and answers. The shell passes the status and body back to
   the module unchanged, and the module shows its own platform's refusal in its
   own UI. A module that knows a person cannot do something may simply not
   offer it; that is the platform's business, not a message to the shell. The
   shell words only its own refusals: outside the prefixes, read-only shell,
   platform unreachable.

6. **No automatic retries.** The shell never retries a write. The module's
   SDK mints an idempotency key per attempt and reuses it if the person
   retries, and the shell forwards it as `Idempotency-Key`. A platform that honours it makes a retry after a lost
   answer safe; one that does not is no worse than a double click.

7. **A failed write changes nothing the shell holds.** The panel's state is
   the module's to show; the shell's frame and state around it are unaffected.

8. **Success is announced, not inferred.** A module that wrote something sends
   `changed`, and the shell relays it to that platform's other mounted modules
   on every surface it serves. Changes made elsewhere (another shell, the
   platform's own frontend) arrive on the platform's event stream
   ([[HLIN-A-0011]]), relayed the same way.

9. **Shell-drawn panels can write too.** A panel drawn by the shell from data
   (the data tier) posts through `Intent::Act`, naming a path under the same
   prefixes, and gets the same carriage.

## Alternatives Analysis

| Option | Pros | Cons | Risk | Cost |
|--------|------|------|------|------|
| Per-action manifest declarations with typed inputs (first draft) | Diffable per action; the shell validates inputs | The shell would draw forms for UI the platform ships itself; a vocabulary of field types to govern. Superseded by [[HLIN-A-0014]] | Low | M |
| Requests under declared prefixes, carried over the bridge with bound identity (chosen) | No address arrives from outside the platform's claim; identity rests on which frame asked; the platform keeps its own UI and validation | Coarser than per action: anything under the prefix can be called | Low | S |
| Browser calls platforms directly with a shell-issued token | Least shell code | CORS on every platform; a bearer token in page script; breaks "the browser only talks to the shell" | High | S |
| Generic write passthrough `/api/platforms/{id}/*` | Any write a platform supports, no declaration | The shell becomes a proxy for its own credential and forwards things it cannot test or degrade from. Rejected in [[HLIN-I-0005]] for options already | High | S |
| Platforms return per-row permission hints so the UI hides what would be refused | Fewer refused attempts | A second authorization channel back into the shell; platforms describe authz rather than enforce it. Rejected in [[HLIN-I-0010]] design | Medium | S |
| Same unbound token for reads and writes | No identity change | A captured read token can perform any action on the same platform within its lifetime | Medium | XS |

## Rationale

The options route already proved the shape: the shell looks up what a platform
claimed instead of trusting an address it was handed. With platforms shipping
their own UI as modules ([[HLIN-A-0014]]), the claim is a prefix rather than
a list of actions, because the module and the endpoints behind it ship
together.

Keeping authorization entirely on the platform is what lets two platforms with
unrelated rules sit side by side without either knowing Hlin's opinion of the
person, because Hlin has none. The cost is that a person can be offered an
action they are not allowed to take, and that cost is paid in a clear refusal
in the platform's own words.

Binding the token to the request is cheap here because the shell mints a token
per request already, and it matters more for writes than it ever did for reads:
a replayed read reveals what the person could already see, while a replayed
write changes something in their name.

## Consequences

### Positive
- People can change things on the platforms they compose, without the browser
  ever talking to a platform.
- Two platforms can hold entirely different authorization models behind one
  surface, and Hlin stays out of both.
- Nothing new drives freshness: an action is one more reason for a panel to
  count as changed.

### Negative
- A person may be offered an action and then refused. The UI has to make
  refusals read well.
- A prefix is coarse: the shell cannot tell one write from another under it,
  so everything it guards is the platform's to guard.
- Platforms that want actions must verify `htm`/`htu`. `hlin-identity` does it,
  but a platform verifying by hand has one more thing to get wrong.
- `static-bearer` and `none` platforms cannot take actions at all, by design.

### Neutral
- Amends [[HLIN-S-0001]] (route prefixes), [[HLIN-S-0004]] (bound tokens,
  `email` from `oidc`), and [[HLIN-I-0005]]'s closed intent vocabulary
  (`Intent::Act`, for the data tier).
- Revised on 2026-09-24 to follow [[HLIN-A-0014]]. The first draft declared
  each action with typed inputs in the manifest; the second carried
  hypermedia posts. Writes now come from sandboxed modules over the bridge.
