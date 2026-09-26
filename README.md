# Hlin

**One place to work across many systems, assembled by the people who use them.**

Hlin is named for the Norse goddess who watches over those she is named to protect.

---

## Running it

The image carries the shell and a front end together; the release tarballs carry
the binary alone, which serves the API and has nothing to look at.

```sh
docker run --rm -p 8080:8080 \
  -v hlin-state:/home/hlin/state \
  -v ./hlin.toml:/etc/hlin/hlin.toml \
  ghcr.io/colliery-io/hlin:v0.0.1
```

Two things it will tell you about on the way up, both deliberate:

- **It refuses the `dev` authenticator.** That strategy makes every request the
  same person *and* lets that person write, so a release build will not start
  with it. Use `anonymous` for an open instance, `trusted-header` behind a
  proxy that authenticates, or `oidc`. See `docker/hlin.toml`.
- **It refuses to start without a database.** The surfaces people compose live
  in Postgres. Without one they are lost on the next restart, along with the
  links anybody shared — the thing Hlin is for, failing quietly, which is worse
  than failing to start. A debug build still falls back, and says what is lost.

### No identity provider

An open ecosystem has no provider to point at and no reason to acquire one.
`anonymous` is the strategy for it, and it needs nothing:

```toml
[auth]
strategy = "anonymous"
```

Every visitor gets their own identity from a cookie the shell sets — not one
shared anonymous principal, because a surface is keyed by its viewer and a
shared one would have one person's click move everybody else's charts.

**A shell using it refuses every write**, and the front end stops offering Edit
rather than offering it and failing at Save. That is what makes anonymity safe
rather than merely convenient: nobody signs in, so nothing can be owned, and a
visitor able to edit could delete the surfaces everyone else came to see.
Read-only is not a separate setting — `anonymous` plus writes is `dev`, which
is refused above for that exact reason.

So an open instance shows what has been *published*. Compose against the same
database from a shell configured with `trusted-header` or `oidc`, publish, and
point the open one at it.

Platforms in an open ecosystem often want no credential either:

```toml
[[platforms]]
id = "public-thing"
base_url = "https://data.example.org"
auth = { strategy = "none" }
```

### Behind a proxy

Configured with `trusted-header` and no proxy in front, every request is
refused and the browser says so. That is the strategy working, not a fault. To
look around without standing up a proxy, send the header yourself:

```sh
curl -H "x-forwarded-user: you" http://localhost:8080/api/platforms
```

For a browser, put any header-setting proxy in front — or run the demo below,
which uses the `dev` authenticator that a debug build allows and a release
build refuses.

### An identity provider

For a real identity provider, use `oidc`. Register `{public_url}/auth/callback`
as the redirect URI — it is derived rather than configured, so it cannot drift
from the route that exists:

```toml
[auth]
strategy = "oidc"
issuer = "https://id.example.com"
client_id = "hlin"
client_secret_env = "HLIN_OIDC_CLIENT_SECRET"
public_url = "https://hlin.example.com"
```

To try it without a provider of your own, `angreal demo up --with collab` runs
the shell on `oidc` against [Dex](https://dexidp.io) in a container (below).

### An internal certificate authority

The shell reaches everything over TLS — every platform it fronts, every event
stream it subscribes to, and the identity provider — and internal services are
usually served from an internal CA. Name its bundle and every one of those
connections trusts it:

```toml
ca_bundle = "/etc/hlin-trust/ca.pem"
```

It is merged with the host's own trust store rather than replacing it, so
platforms on an internal CA and a provider on a public one need no choice
between them. A bundle that cannot be read, or that contains no certificates,
stops the shell at startup: trusting nothing extra would fail later against
every platform at once and look like an outage rather than a typo.

In the chart, `config.caBundle` takes the PEM inline, or the name of a
ConfigMap or Secret that already holds one — cert-manager writes its CA into a
Secret, so that case is common:

```sh
--set config.caBundle.existingSecret=corp-ca --set config.caBundle.key=ca.crt
```

On Kubernetes, there is a chart:

```sh
helm install hlin oci://ghcr.io/colliery-io/charts/hlin --version 0.0.1 \
  --set config.auth.trustedHeader.acknowledgeProxyRequired=true \
  --set config.databaseUrlSecret.name=hlin-db \
  --set config.databaseUrlSecret.key=url
```

It brings a Postgres by default, so this produces something that can actually do
what Hlin is for rather than a degraded version of it. Turn that off and point at
your own for anything whose loss would matter.

It refuses rather than renders what would fail quietly, each refusal mirroring
one the shell itself makes and for the same reason:

- No database at all, which is now an install that cannot start.
- `dev` as an authenticator, which a release build will not start with anyway.
  `anonymous` is offered instead, and needs nothing configured.
- `trusted-header` without acknowledging what it trusts. A pod IP is reachable
  from the rest of the namespace by default, so an ingress in front is not on
  its own enough.
- More than one replica without a shared signing key. Each would otherwise
  generate its own, and a token minted by one pod fails to verify against
  another's key set — one request in two, rather than all of them.
- Two trust bundles at once. Only one can be mounted, so the others would be
  ignored in silence — and a trust anchor that is silently ignored fails
  against every platform at once.

### Modules that stream: serve the shell over HTTP/2

A module may read a response as it arrives — a log tail, a feed, a long
export — and every such stream holds a request from the browser to the shell
open for as long as it runs. Over HTTP/1.1 a browser opens about six
connections to one origin, and the shell's own event stream needs one of them,
so the page holds at most **4** module streams at once across the whole
surface and refuses the next. Over HTTP/2 or later every request shares one
connection, and the page allows **32**. The page reads which it was served
over from the browser, so nothing is configured: put the shell behind
something that speaks HTTP/2 to browsers (any TLS-terminating ingress or load
balancer does) if its platforms' modules stream. Browsers speak HTTP/2 only
over TLS, so a plain-`http` shell, like the demo's, is always on the cap of 4.

Each stream is also bounded by the shell, per platform if need be:

```toml
[modules.limits]
streams = 2                      # open at once, per frame; 0 turns streaming off
stream_bytes_per_second = 1048576
stream_idle_seconds = 60         # ended when nothing arrives for this long

[[platforms]]
id = "logs"
base_url = "https://logs.internal.example.com"
modules.limits = { stream_idle_seconds = 300 }
```

A streamed response is not bounded by `response_bytes`, and the upstream
timeout covers it only until the platform sends its headers.

### Bringing your own design pack

Panels are drawn by whichever design pack the front end was built with, and the
image ships one. To draw them with yours, write a pack — anything implementing
`DesignPack` from [`hlin-view`](https://crates.io/crates/hlin-view) — and a
front end that mounts it:

```rust
use hlin_ui::app::App;
use leptos::prelude::*;
use your_pack::YourPack;

fn main() {
    leptos::mount::mount_to_body(|| view! { <App pack=YourPack /> });
}
```

`trunk build --release` produces a `dist`. Put it in an image — a few megabytes
on `busybox`, no Rust toolchain in it — and name that image:

```dockerfile
FROM busybox
COPY dist /dist
```

```sh
helm install hlin oci://ghcr.io/colliery-io/charts/hlin \
  --set config.auth.strategy=anonymous \
  --set config.frontendImage=ghcr.io/you/your-pack:1.0
```

An init container copies it over the front end the shell image was built with,
so the shell stays upstream's: a fix to it is a tag bump rather than your
rebuild. If you would rather have one image, `FROM ghcr.io/colliery-io/hlin`
and `COPY dist /home/hlin/frontend` does the same thing.

Then point it at your platform — anything serving a manifest at
`/.well-known/hlin.json`:

```yaml
config:
  platforms:
    - id: your-platform
      baseUrl: https://your-platform.internal
      auth:
        strategy: hlin-token     # or `none`, for a platform that asks for nothing
```

To work on it instead, everything is an `angreal` task:

```sh
angreal demo up        # database, front end, three sample platforms, the shell
angreal demo compose   # a surface worth looking at, and its URL
angreal demo down
```

`angreal tree` lists the rest. `.metis/` holds the decisions this was built
from; the ADRs are why the code is shaped the way it is and are worth reading
before changing it.

---

## The problem

We are building roughly a dozen Rust platforms. Each has a Leptos frontend served from its own root, each ships on its own cadence, and each is developed by a team that should not have to coordinate with the other eleven to release.

The cost lands on the people using them. Their work is spread across a dozen origins, so anything that spans two systems means holding two tabs, signing in twice, and doing the correlation by hand. There is no single place to see how things are, and no single place to act on it. Each frontend is coherent on its own; together they are twelve tools rather than one product.

The obvious fixes are both wrong. Building a thirteenth hand-written application puts every cross-cutting need behind a central team's backlog and goes stale the moment a platform changes. Composing the frontends naively, as a dozen WASM binaries sharing one document and one authority, makes every platform's bug everyone's outage and puts every platform's credentials in the browser.

## What Hlin is

Hlin is a workspace shell.

Each platform declares, at runtime, what it offers, and ships its own UI for it as modules. Hlin discovers those declarations, runs each module in its own sandbox, and lets a person select panels and pages from any platform and arrange them into a surface they authored. Where a platform offers data rather than a module, Hlin draws it itself.

Platforms own their data, their rules and their UI. Hlin owns the page, the person and the wire: the only code in the shell's page, the signed-in identity, and every request between a module and its platform.

That split produces most of the properties we want. A platform ships UI at its own pace, with no ceiling on what it can do. A module failing is one panel in a known state, never a broken page. The browser never holds a credential, because every request goes through the shell, which binds the viewer's identity to it and lets the platform decide. Adding a platform costs the shell nothing, because the shell was never compiled against it.

## What Hlin is not

**Not a reverse proxy.** The shell carries requests between modules and platforms, and that plumbing is necessary, but it is not the product. It carries only what a platform declared, only for the platform that owns the module asking.

**Micro-frontends, isolated.** Independently built modules, each in its own sandboxed frame with an opaque origin, reaching nothing but their own platform, and only through the shell. Never in the shell's page.

**Not a query layer.** Hlin shows what platforms choose to expose. It does not reach into anyone's database, and it defines no query language over platform internals.

**Not a general dashboarding tool.** Grafana exists and is better at being Grafana. Hlin hosts the UI of the teams that own the systems, in the terms of those systems.

**Not a central bottleneck.** Shipping a panel or a module does not require a Hlin release, a Hlin PR, or a design review.

**Not an authorization system.** Hlin knows who a person is. What they may do is each platform's decision, made from the identity the shell forwards and whatever the platform keeps about them.

## Principles

**Platforms ship their UI; the shell hosts it.** A platform decides what its UI looks like and does. Consistency comes from a shared kit every module builds with and theme tokens the shell sends in.

**No platform code in the shell's page.** Every module runs in a sandboxed frame with an opaque origin. It cannot reach the shell's page, its cookies, another module, or the network.

**Every request goes through the shell.** A module holds no credential. The shell carries its requests, only to its own platform, only under paths that platform declared, with the viewer's identity bound to each one.

**The platform decides who may do what.** Hlin authenticates and forwards; it holds no roles and interprets nobody's policy.

**Platforms are autonomous.** Adding a platform, a panel, a module, or a navigation entry is a deploy of that platform. Discovery happens at runtime against a manifest each platform serves about itself. Hlin is never rebuilt to accommodate a child.

**The manifest and the bridge are public APIs.** Panel keys, pages, parameters, route prefixes and the bridge protocol are a declared contract, not an implementation detail. They are versioned and deprecated with a window and a named successor. Removing a panel key without a major version is a breaking change; the shell detects it at runtime and flags it, and no build anywhere has to fail for the contract to be enforced.

**Vocabulary is additive and governed.** The view kinds the shell draws are a bounded set owned by the design system. Unknown kinds degrade to a fallback; they never break a layout.

**Composition belongs to users.** Selecting and arranging panels requires a deploy from nobody. The set of useful surfaces is not knowable in advance by any central team, so we do not try to enumerate it.

**Rendering is total over its inputs.** Every panel is in exactly one state at all times: loading, ready, stale, or unavailable, with unavailability distinguishing unreachable from malformed from unknown from deprecated. The shell draws the frame and state around every panel, so there is nothing a platform or a module can do, including never answering, that the shell has no rendering for.

## Architecture

### The manifest

Each platform serves a document describing itself at a well-known path: navigation entries, offered panels and pages, their modules or data endpoints, the parameters each responds to, the route prefixes the shell may carry requests to, health and summary endpoints, and lifecycle status for anything deprecated. Three rules keep it survivable across a dozen independent release cadences: the schema version is monotonic and additive with unknown fields ignored; icons and view kinds are names from a shared vocabulary rather than code; a malformed or absent manifest degrades to a plain link with a warning indicator, never a shell error.

### Discovery

Hlin reads a platform list from runtime configuration and polls each manifest. The registry sits behind a trait, so a push-based model with heartbeat expiry, or orchestrator-native label discovery, can replace configuration later without reshaping anything above it.

### Modules

A platform's own UI for a panel or a page, built with the shared kit and the shell's SDK, served through the shell and run in a sandboxed frame. It talks to the shell over a versioned bridge: context in (time range, parameters, theme, who is signed in), requests and intents out. Decision HLIN-A-0014.

A module must never block its main thread. The sandbox contains what a module can reach, not how long it runs: in Firefox, WebKit and Chromium's headless shell a module's frame shares the page's thread, so a module that spins holds the whole surface still until it stops. Work that takes more than a frame or two belongs in chunks that yield, or in a Web Worker loaded from the module's own assets. A debug build of a module made with `hlin-module` warns in its console when a task holds the thread for 200 ms or more; a release build does not watch.

### Identity and requests

A person signs in to Hlin once. Every request a module makes goes through the shell, which binds the viewer's identity to it, and the platform decides what to allow. Decisions HLIN-A-0004 and HLIN-A-0013.

### Panels drawn by the shell

A panel may instead declare a view kind and a data endpoint returning a typed envelope, and the shell draws it. This is the way to show cross-platform summaries and time-driven charts, and the fallback when a module cannot load. Fan-out is aggregated shell-side, deduplicated across panels requesting the same data, and delivered over a single stream.

### Shared context

Shell-level controls, notably time range, drive every panel and module that declares the corresponding parameter, whoever drew it.

### Views

The view registry for shell-drawn panels lives in a companion crate beside the design system, so the component library stays free of Hlin's vocabulary.

### Customization

Users choose panels and pages, arrange them, and set titles and time ranges. A platform that needs something bespoke ships it in its own module; nobody waits on a vocabulary change.

## What we are betting on

That sandboxed modules stay light and consistent enough to feel like one product. Each panel on screen costs a document and a runtime, and consistency now rests on a shared kit and convention rather than on the shell drawing everything. If surfaces load slowly, or teams' UIs visibly drift apart, the answer is shared caching and a stricter kit, not loosening the sandbox. Watch both in the first two adoptions.

## Success condition

A team ships a new module on Tuesday morning. Someone on another team has it on their surface Tuesday afternoon, next to panels from two other platforms, with no Hlin release, no design review, and no conversation between the two teams.

A person does a day's work in a platform without opening its own frontend.

## The design questions, answered

All five are now decided. Each names the decision that closed it.

- **The manifest schema.** A document at a well-known path, with contract identity separated from presentation, so a fingerprint moves only when a promise moves. Specification HLIN-S-0001.
- **The data envelope per view kind, and the initial vocabulary.** Five envelopes and six kinds, named separately and paired by an acceptance matrix, so data shape and rendering evolve on their own cadences. Specification HLIN-S-0002, decision HLIN-A-0003.
- **Where the view registry lives.** A companion crate, `hlin-view`, so the design system stays a plain component library that platform frontends can use without taking on Hlin's contract types. Decision HLIN-A-0005.
- **Layout persistence and sharing.** One owner per layout, personal or published to a shell-wide gallery, shared read-only with fork to edit. A layout referencing a platform the viewer cannot reach renders normally, with the refused panels showing as forbidden. Decision HLIN-A-0007.
- **Authorization.** Authentication is hoisted to Hlin, the shell forwards a signed identity on every request, and platforms decide at fetch time. The shell interprets nobody's policy. Decision HLIN-A-0004, specification HLIN-S-0004.

What remains open is recorded in those specifications rather than here.

## Crates

```
crates/
  hlin                  shell binary: discovery, registry, aggregation, layouts, store
  hlin-manifest         contract crate: schema, panel entries, lifecycle, envelopes
  hlin-view             view registry: kinds, the acceptance matrix, and DesignPack
  hlin-stream           the wire types the shell and the browser share
  hlin-identity         minting, publishing and verifying the forwarded identity
  hlin-ui               the composition machinery, generic over its design pack
  hlin-pack-demo        a design pack, implementing the rendering interface
  hlin-sample-platform  the reference platform a real one is copied from
  hlin-bridge           the messages between the shell's page and a module
  hlin-module           the SDK a Leptos module is built with
  hlin-sample-checklist a platform people change, with its own module and rules
  hlin-sample-feed      a second one, deciding who may write from their claims

examples/
  frontend-demo         hlin-ui mounted with the demo pack
  frontend-aurora       hlin-ui mounted with Aurora Dark
```

Nothing in `crates/` names a design system. A front end is a binary that picks a
pack and mounts `hlin-ui`, which is why there are two of them and why they
differ by one line.

Dependency direction is `hlin` → `hlin-view` → `hlin-manifest`. The contract
crate depends on nothing else in the workspace, and the design system stays
outside all three.

`hlin-stream` exists because the browser cannot take the shell's dependencies:
`sqlx`, `tokio` and `axum` do not build for `wasm32`. Putting the wire types in
a crate of their own means the shell and the browser read one definition rather
than two that drift.

`hlin-bridge` is the same idea for modules, and is kept apart from
`hlin-stream` so a platform team building a module takes on `serde` and
nothing of the shell's.

## Try the demo

Two sample platforms, the shell, and a frontend you can compose a surface in.

**Prerequisites:** Rust 1.93, Docker (for the development database), and
[`trunk`](https://trunkrs.dev) with the WebAssembly target:

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

**Two commands:**

```bash
angreal demo up           # database, frontend, two platforms, the shell
angreal demo walkthrough  # drives it and asserts what should happen
```

`up` prints a URL. Opening it lands you on a surface of your own, empty the
first time. Press **Edit** to open the picker on the left, click a panel to put
it on the grid, then drag it by the handle in its heading and resize it from the
corner. Every arrangement is written as you make it, so a reload brings back what
you left. The time picker at the top drives every panel that declared it
responds to a time range, which is eight panels from two platforms answering one
control without any of them knowing about each other.

Two platforms run because one is not enough to show the point. Stopping one and
watching its panels degrade while the other keeps working is the difference
between a shell that composes and a page that breaks.

`angreal demo walkthrough` is that argument, made without a person watching. It
composes a surface from both platforms, changes the time range, kills one
platform, brings it back having dropped a panel without a major version bump,
and puts it right again, asserting the shell's behaviour at each step. It exits
non-zero and names the first thing that did not hold.

`angreal demo status` says what is running; `angreal demo down` stops it. Logs
are under `demo/state/logs/`. If a database is already listening on 55432, `up`
leaves it alone rather than starting a second one.

### Seeing it without watching it

The walkthrough proves the shell degrades correctly. It says nothing about what
is on a screen. That is what the browser tests are for:

```bash
angreal e2e install   # once: Playwright and the browser it drives
angreal e2e test      # against a running demo
```

They drive a real browser through composing a surface — opening the picker,
adding panels from both platforms, dragging one, resizing it, switching how a
panel is drawn, reloading — and through losing the stream and getting it back.
Each step leaves a screenshot in `e2e/screenshots/`, numbered so the sequence
reads in order, which is the fastest way to see what the product currently looks
like. `angreal e2e test --headed` runs it in a browser you can watch.

They work in layouts they create and delete through the API, so a run neither
depends on nor disturbs a surface you composed. They also avoid asserting on any
design pack's markup, so the same suite runs against any front end.

### Signing in as somebody: the collaborative demo

The standard demo makes everybody the same development user, looking at
platforms nobody can change. The collaborative one signs people in through a
real identity provider, in front of two platforms that accept writes and
decide for themselves who may do what:

```bash
angreal demo up --with collab   # Postgres, Dex, both platforms and their modules, the shell on `oidc`
angreal e2e signin              # signs in through Dex's form; adds, ticks and posts as Alice, Bob and Carol
angreal e2e walkthrough         # Alice and Bob in two browsers at once; Carol in a third
```

Open `http://127.0.0.1:8080` — `127.0.0.1`, not `localhost`, because the
session cookie belongs to the host name the browser used — and sign in as
`alice@example.com`, `bob@example.com` or `carol@elsewhere.org`, password
`password`. Your name is in the bar, beside **Sign out**.

The story so far:

- **Two platforms, their own rules.** `hlin-sample-checklist` (port 8083)
  keeps shared lists with owners and members: Alice owns `team`, Bob is on it,
  Carol has a list of her own. `hlin-sample-feed` (port 8084) is a feed anyone
  signed in may read and only people at `example.com` may post to. Both verify
  the shell's signed token (`hlin-token`) and decide from its `email`; the
  shell knows none of their rules.
- **One published surface.** `up` signs in as Alice through Dex — the same
  journey a browser makes, through the shell's ordinary API — and publishes
  **The team**: the checklist on the `team` list beside the feed. Alice lands
  on it when she signs in. Running `up` again replaces it rather than adding
  another.
- **Each platform draws itself.** Both panels are the platforms' own Leptos
  modules (`crates/hlin-sample-checklist/module`, `crates/hlin-sample-feed/module`),
  built with Trunk by `up` and hosted by the shell in sandboxed frames. Tick,
  add, edit and delete items; post, edit and delete posts. Every change goes
  from the module through the shell to its platform, as the person signed in,
  and what the module shows is what the platform then says. Where a module
  cannot load, the shell draws the same panel as a table.
- **The same surface, not the same for everyone.** Anyone signed in can open
  it (its link is printed by `up`). Bob sees what Alice sees, and may tick her
  items but not edit them, so his module does not offer to. When Bob tries to
  edit Alice's post, the feed refuses, and its module shows the feed's own
  words. Carol reads the posts but is refused when she posts, and the
  checklist refuses her the team list — each platform's decision, in its own
  words, shown rather than hidden.
- **Live across browsers.** A change reaches everyone else's open page
  without a reload: the platform says so on its event stream, the shell
  relays it down each surface's stream, and each module fetches again. Sign in
  as Alice and as Bob in two browsers; Alice adds an item and Bob sees it,
  Bob crosses it off and Alice sees it crossed. `angreal e2e walkthrough`
  does exactly that, and prints how long each change took to arrive.
- A first-time Bob or Carol lands on an empty surface of their own and opens
  The team by its link.

Dex is configured by `demo/dex.yaml` and the shell by `demo/hlin-collab.toml`.
Its issuer is plain http on loopback, which only a debug build of the shell
accepts. `up` generates the client secret and hands it to both through
`HLIN_DEMO_OIDC_SECRET`, unless that is already set. `angreal demo down` stops
the platforms and Dex too. `angreal e2e test` expects the standard demo and says so if pointed at
this one; `angreal e2e signin` and `angreal e2e walkthrough` expect this one.

### The containerised twenty

`angreal demo up --with twenty` runs twenty small widget platforms as
processes on ports 8201 to 8220, which is quick to work on. `--with
twenty-compose` runs the same twenty the way they are meant to be deployed:

```bash
angreal demo up --with twenty-compose   # build the images, start them, publish "Twenty"
angreal demo down                       # stop them
angreal demo down --keep-database       # stop them, leaving the development database up
```

**This is the reference for how a platform should serve Hlin.** A platform
keeps its own UI at the root of its own host and gives Hlin a subtree of it:

- `/` is the platform's own UI, a client-side app behind a catch-all
  fallback, as any single-page app is served.
- `/hlin` is Hlin's, and only Hlin's: the manifest at
  `/hlin/.well-known/hlin.json`, the module's build, the data routes the
  module calls (verified with the shell's `hlin-token`), and the event stream.
  An unknown path under it is a 404, never the own UI's `index.html`. The
  shell's `base_url` for the platform is `https://<host>/hlin`.
- **The Hlin module is not a second UI.** Each widget has one components
  crate; its own UI mounts those components with a client that calls the
  platform's own `/api/`, and its module re-exports the very same components
  with a client that goes through the bridge. A change in either shows in the
  other, through nothing but the platform.

`crates/widgets/*` are twenty worked examples, and `deploy/twenty/` deploys
them that way.

- **One container per widget**, each on its own name on a compose network
  (`clock.comp.test` to `meetings.comp.test`), serving its own UI at `/` and
  Hlin under `/hlin`, with both builds compiled into its one binary. None is
  published: the shell reaches them at `http://<name>.comp.test:8080/hlin`.
- **The shell is a release build** (`hlin.comp.test`), published on
  `http://127.0.0.1:8090` and nothing else, signing people in through a Dex
  and keeping layouts in a Postgres of the stack's own: project `hlin-twenty`,
  leaving the development database, the collaborative demo's Dex and 8080
  alone.
- **Dex is on https**, because a release shell will not trust a plain-http
  issuer. Its issuer is `https://dex.localhost:5557/dex`, one address that
  works from both sides: a browser resolves any `*.localhost` to loopback,
  where Dex is published, and the shell's container resolves it on the compose
  network, where it is Dex's alias. `up` makes a throwaway CA for it under
  `demo/state/twenty/tls`, which the shell trusts through `ca_bundle`; your
  browser warns once unless you trust `ca.pem` too.
- `up` signs in through Dex as Alice and publishes **Twenty**, all twenty
  widgets on one surface; sign in as `alice@example.com` (or Bob, or Carol),
  password `password`.
- **A platform going down and coming back** is a container stopped and
  started:

  ```bash
  docker compose -f deploy/twenty/compose.yml -p hlin-twenty stop clock
  docker compose -f deploy/twenty/compose.yml -p hlin-twenty start clock
  ```

Every image comes from one builder stage in the repository's `Dockerfile`
(`TWENTY=1`), which compiles the shell, its front ends, every module and own
UI (release, `wasm-opt`) and every widget server once, with cargo's registry
and target directory on BuildKit cache mounts. The first build takes a while;
after that an unchanged build is seconds and an edit recompiles what it
touched. The files are `deploy/twenty/`: the compose file, the shell's
configuration, and Dex's.

The twenty suites run against it as they do against the processes:

```bash
angreal e2e twenty --against compose           # every widget ready, in step, own UIs at their roots
angreal e2e twenty-measure --against compose   # the numbers, and dice stopped and started twice
```

Both sign in through Dex as Alice first (Playwright ignores the throwaway
CA). The widgets' own UIs are published nowhere, so `e2e twenty` opens them by
their names, `http://clock.comp.test:8080/`, through a small proxy it runs in
a container on the compose network for the length of the suite
(`e2e/network-proxy.js`); so `/`, and a 404 for `/hlin/nope`, are checked from
inside the network, where the shell reaches them. Either refuses to run
against the wrong stack.

### Seeing it in a real design system

The demo pack exists so this repository can demonstrate itself while depending
on nothing published. It is deliberately plain and is not a design system.

`examples/frontend-aurora` is the same front end drawn by Colliery's Aurora
Dark, which is a real one — taken from crates.io like any other dependency,
with its own `hlin` feature turned on:

```bash
angreal ui build --which frontend-aurora
./target/debug/hlin serve --config demo/hlin-aurora.toml
```

The two binaries differ by one identifier. Choosing a design system is a
dependency and a line; everything else is `hlin-ui`, which has no idea what is
drawing its panels. Nothing in `crates/` mentions Aurora, and Aurora's own
default build does not mention Hlin — the feature that makes it a pack is
additive and off unless asked for:

```toml
colliery-io-aurora = { version = "0.2", features = ["hlin"] }
```

`examples/frontend-gallery` holds both and reads the pack from the address, so
you can see the same surface drawn two ways without a rebuild:

```bash
angreal ui build --which frontend-gallery
./target/debug/hlin serve --config demo/hlin-gallery.toml
# then /s/{layout}?pack=aurora  and  /s/{layout}?pack=demo
```

That one is a demonstration rather than a deployment pattern. Both design
systems are compiled into the binary and the bundle is about a megabyte larger
for it. It exists to show the seam is real; ship one pack.

## Development

This project uses [angreal](https://github.com/angreal/angreal) for task automation.

### Prerequisites

- Rust 1.93+
- [angreal](https://github.com/angreal/angreal) (`pip install angreal`)
- [pre-commit](https://pre-commit.com/) (`pip install pre-commit`)
- Docker, for the development database (`angreal db up`)

### Common Commands

```bash
# Development database (the shell's store is Postgres)
angreal db up
angreal db status
angreal db reset

# Run checks
angreal check all

# Run tests
angreal test unit
angreal test integration
angreal test coverage

# Build
angreal build
angreal build --release

# Version management
angreal version show
angreal version bump patch
```

## License

MIT
