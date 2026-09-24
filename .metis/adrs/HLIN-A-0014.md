---
id: 001-hlin-is-where-people-work
level: adr
title: "Hlin is where people work: platforms ship their own UI as modules, each sandboxed in its own frame, and the shell owns the page, the person and the wire"
number: 1
short_code: "HLIN-A-0014"
created_at: 2026-09-24T21:29:39.013678+00:00
updated_at: 2026-09-24T22:22:27.363145+00:00
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

# ADR: Hlin is where people work: platforms ship their own UI as modules, each sandboxed in its own frame, and the shell owns the page, the person and the wire

## Context

The vision rests on one bet: *the shell renders everything.* Panels cross the
boundary as data plus a view kind from a closed vocabulary, so visual
consistency is structural. It says what Hlin is not in the same breath: "Not
micro-frontends. No runtime composition of independently built WASM modules;
not now, not as a later phase."

Designing [[HLIN-I-0010]] (a signed-in person changing things on two
platforms) brought the purpose into question. Asked directly, the owner chose:

| Question | Answer |
|---|---|
| What is Hlin for? | **The place people work.** The main UI for many platforms, not a vantage over them |
| Who decides how a panel looks? | **The platform.** The shell provides layout, identity and transport |
| Who are the platforms? | **Only ours.** A dozen internal teams |
| Where are we headed? | **Micro UI.** Platforms ship their own UI code |
| How is that code isolated? | **A sandboxed iframe per panel** |

A server-rendered hypermedia model was drafted first, to keep platform code out
of the page. It was withdrawn. The goal is micro UI, and building a hypermedia
runtime to replace later would be a detour.

The vision's reasons for ruling micro-frontends out were real, and this
decision has to answer each one rather than set it aside:

| Objection | Answer here |
|---|---|
| A dozen WASM runtimes in one document is not viable | Overstated: heavy, not unviable. Modules load only when their panel is on screen, are cached by content hash, and each runs in its own frame rather than sharing one document |
| The browser would need platform credentials | It never gets any. A module cannot reach the network; it asks the shell's page, which asks the shell, which asks the platform with the viewer's identity bound to that request |
| One team's bug breaks every surface | Each module is in a sandbox with an opaque origin. It cannot touch the shell's page, its cookies, or another module. A crash is one panel showing `unavailable` |
| Visual consistency is lost | By convention now: a shared component crate every module builds with, and theme tokens the shell sends in |

## Decision

**Platforms ship their own UI as modules. The shell runs each one in a
sandboxed iframe and owns everything around it: the page, the person, and
every request between a module and its platform.**

### Who owns what

| Platforms own | The shell owns |
|---|---|
| Their data | **The page**: the grid, the chrome, the frames, and states around them |
| Their rules: who may do what | **The person**: sign-in, sessions, identity |
| Their UI: modules, built and released on their own schedule | **The wire**: every request a module makes, with identity bound to it |
| Their endpoints, reads and writes | **Composition**: the surfaces people assemble, and pages |
| | **Shared context**: time range, parameters, theme, sent to every module |
| | **Liveness**: event subscriptions, relayed to the modules that care |
| | **Policy**: sandboxing, CSP, which paths a module may reach, limits |

### Rules that do not bend

1. **No platform code runs in the shell's page.** Every module runs in an
   iframe sandboxed without `allow-same-origin`, so it has an opaque origin: no
   cookies, no storage shared with the shell, no access to the parent's
   document or to another module.
2. **A module cannot reach the network.** Its CSP allows loading its own
   assets and nothing else. Every request goes over the bridge to the shell's
   page, which knows which frame sent it.
3. **Every request to a platform is made by the shell**, with the viewer's
   identity bound to that request ([[HLIN-A-0013]]).
4. **The platform authorizes. Hlin authenticates**, and sends and receives no
   authorization information.
5. **A module can reach only its own platform, and only paths that platform
   declared.** Identity comes from which frame sent a message, which a module
   cannot forge. The path is checked against the platform's declared
   prefixes.
6. **Rendering stays total.** The shell draws every panel's frame and state.
   Loading, ready, stale and unavailable still hold whatever a module does,
   including never answering.
7. **The shell is never rebuilt for a platform.**

### Mechanism

**Declaring a module.** A panel, or a page reached from a navigation entry,
declares `ui`: the entry document of its module, under a declared asset
prefix, and the major version of the bridge protocol it speaks. `data` and
`kind` become optional. A panel declaring both a module and data falls back to
the shell drawing the data when the module cannot load.

**Serving it.** The shell serves module assets from its own origin, under
`/m/{platform}/…`, proxied from the platform's asset prefix. It caches them by
content hash and serves them with a CSP of its own (scripts,
`wasm-unsafe-eval` and connections limited to that module's assets). The
frame is `sandbox="allow-scripts allow-forms"`. Popups and top-level
navigation are off, and navigating is a bridge message.

**The bridge.** A versioned `postMessage` protocol. The shell's page accepts a
message only when its source is a frame it created, and knows the platform,
panel and instance from that alone:

| Shell to module | Module to shell |
|---|---|
| `init`: protocol, context, theme, who is signed in (display only) | `ready` |
| `context`: time range and parameters changed | `fetch`: method, path, body; answered with status and body |
| `theme`: tokens changed | `set-param`, `set-range`: the [[HLIN-I-0005]] intents |
| `changed`: the platform says data changed | `navigate`: open a panel or page in the shell |
| `visibility`: shown or hidden, so a module can idle | `changed`: I wrote something; tell the others |
| `response`: to a `fetch` | `notice`: something for the shell to show |

Unknown messages are ignored. A module that does not say `ready` in time, or
stops answering a heartbeat, is `unavailable`.

**Requests.** A module's `fetch` goes to `/p/{platform}/{path}` on the shell,
as the shell's own page, with the session cookie. The shell checks the prefix,
mints a request-bound token, calls the platform, and passes the status and body
back as-is: a module draws its own platform's answers, refusals included. Reads
may be retried by the module. Writes carry an idempotency key the SDK mints,
are never retried by the shell, and are refused on a read-only shell
([[HLIN-A-0012]]).

**Liveness.** Modules cannot hold connections. The shell keeps its one
subscription per platform ([[HLIN-A-0011]]) and relays `changed` to every
mounted module that platform owns. A module's own `changed` is relayed the same
way, to its siblings on every surface this shell serves.

**The SDK.** A Rust crate for Leptos modules that speaks the bridge: context as
signals, `fetch` as an async call, theme applied to the document, `ready` and
heartbeat handled. A module built with it does not know it is in a frame.

**The kit.** Consistency by convention: the shared component crate (Aurora)
compiled into every module, themed by the tokens the shell sends.

**The grid.** Frames swallow pointer events, so the shell lays a shield over
them while a panel is dragged or resized. Frames mount when their panel scrolls
into view.

**The contract shrinks to what the shell depends on.** Contract: panel keys
and page paths (layouts reference them), parameters (the shell's context
drives them), route and asset prefixes, and the bridge protocol's major
version. Not contract: the module's code and its calls to its own platform,
which ship in the same deploy.

### What happens to what exists

- **Envelopes, kinds, packs, the aggregator and the stream** stay, as the way
  the shell draws a panel itself: cross-platform summaries, time-driven
  charts, and the fallback for a module that cannot load.
- **[[HLIN-I-0003]] and [[HLIN-I-0004]]** re-aim: Aurora becomes the kit every
  module builds with, as well as the shell's design pack.
- **[[HLIN-I-0005]]**'s intents become bridge messages as well as pack
  intents.
- **[[HLIN-A-0004]], [[HLIN-A-0008]], [[HLIN-A-0011]], [[HLIN-A-0012]]**
  hold unchanged, and now also govern module requests.
- **[[HLIN-A-0013]]** is revised: writes are module requests through the
  bridge, under declared prefixes.
- **[[HLIN-T-0059]]** is answered: a platform ships whatever component it
  needs, in its own module.

## Alternatives Analysis

| Option | Pros | Cons | Risk | Cost |
|--------|------|------|------|------|
| The shell renders everything (vision as written) | Structural consistency; strongest contracts | Every platform-specific need waits on a vocabulary change; cannot be where people work | High for the new purpose | — |
| Server-rendered hypermedia, no platform code | No platform code in the browser | Caps interactivity; a runtime built to be replaced when micro UI arrives | Medium | L |
| **Modules in sandboxed iframes, bridged through the shell (chosen)** | Full interactivity; crashes, styles and bugs contained per panel; identity model intact | A document and runtime per panel; consistency by convention; a bridge to maintain | Medium | L |
| Modules in the shell's page (web components, WASM in shadow roots) | Lighter; inherits styles | Every module runs with the shell's authority over every platform; one panic or global rule affects all | High | M |
| Modules in frames holding a bearer capability, calling the shell directly | Simpler than relaying every request | A token in module memory to leak; CORS from an opaque origin; the bridge still needed for context | Medium | M |

## Rationale

"The place people work" means each platform's UI, at each team's pace, with no
ceiling on what it can do. That is micro UI, and starting anywhere else builds
something to throw away.

The sandbox is what keeps the parts of the original bet that still matter. The
browser still never holds a credential, every request still passes through the
shell with bound identity, and the platform still decides. A module failing is
still one panel in a known state, never a broken page.

Relaying requests through the parent, rather than giving a module a token,
means identity rests on something a module cannot forge: which frame a
message came from. The price is a hop per request inside the browser, which is
cheap next to the network hop that follows it.

## Consequences

### Positive
- A platform team ships UI in Hlin by deploying their platform: no shell
  release, no vocabulary change, no design review.
- No limit on what a platform's UI can do.
- A broken module is one unavailable panel.
- Identity, authorization, composition and liveness work as they do now,
  across every platform.

### Negative
- **Weight.** A document and a runtime per panel on screen. Load time per
  surface is the number to watch.
- **Consistency by convention.** The kit and review are all that keep two
  teams' UIs alike, and module versions of the kit will drift.
- **A protocol to govern.** The bridge is a public API with a major version,
  like the manifest.
- **Security-critical shell code.** The bridge, the proxy's prefix check and
  the frame policy are where isolation lives.
- **Frames are awkward.** Pointer capture over the grid, focus and keyboard
  across frames, accessibility titles, printing.

### The new bet

That sandboxed modules stay light and consistent enough to feel like one
product. If surfaces load slowly or teams' UIs drift apart visibly, the answer
is shared caching and a stricter kit, not loosening the sandbox.

## Vision changes

Applied to [[HLIN-V-0001]] on 2026-09-24, recorded under its Amendments.

| Where | Now | Proposed |
|---|---|---|
| Tagline | One vantage over many systems, assembled by the people who use them | One place to work across many systems, assembled by the people who use them |
| Product overview | Renders every declared panel through a single design system | Hosts every platform's own UI, each in its own sandbox, and owns the page, the person and the wire |
| Product overview | Nothing crosses the boundary except data and a declared view kind. Hlin does the rendering; platforms do not ship code into it | Platforms ship UI modules. They run sandboxed, reach nothing but their own platform, and only through the shell |
| Not micro-frontends | No runtime composition of independently built WASM modules; not now, not as a later phase | Replaced: **Micro-frontends, isolated.** Independently built modules, each in its own sandboxed frame, never in the shell's page |
| Customization | Embedding a platform's own frontend in a panel is not a supported mechanism | A platform's own module is the primary mechanism; the shell drawing a panel from data is the other |
| Principle | The shell renders everything | **Platforms ship their UI; the shell hosts it.** Consistency comes from a shared kit |
| Principle | (none) | **The platform decides who may do what.** Hlin authenticates and forwards; it holds no roles |
| Principle | (none) | **Every request goes through the shell**, with identity bound to it. A module holds no credential |
| Principle | Rendering is total over its inputs | Kept: the shell draws every panel's frame and state, whatever a module does |
| Constraint | No runtime composition of independently built WASM modules, in any phase | No platform code in the shell's page, in any phase. Modules run only in sandboxed frames |
| Constraint | Panels cross as data plus a declared view kind only | Panels cross as a module, as data, or both |
| Success criteria | A panel shipped Tuesday is on a dashboard Tuesday afternoon | Kept, plus: a person does a day's work in a platform without opening its own frontend |
| The bet | The vocabulary is a smaller cost than coordination | Sandboxed modules are light and consistent enough to feel like one product |

## Open questions

- **Prefix granularity**: one read and one write prefix per platform, or per
  panel and page.
- **Streaming responses over the bridge**: whether `fetch` needs a streaming
  form, or `changed` plus refetch is enough.
- **Kit drift**: whether the shell should refuse or flag a module built against
  a kit version too far from its own.
- **Budgets**: a per-surface limit on mounted frames, and what the shell shows
  past it.
- **Platforms' own frontends**: whether they keep running beside Hlin or shrink
  into modules. Nothing here requires either.
