# Hlin

**One vantage over many systems, assembled by the people who use them.**

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

To work on it instead, everything is an `angreal` task:

```sh
angreal demo up        # database, front end, two sample platforms, the shell
angreal demo compose   # a surface worth looking at, and its URL
angreal demo down
```

`angreal tree` lists the rest. `.metis/` holds the decisions this was built
from; the ADRs are why the code is shaped the way it is and are worth reading
before changing it.

---

## The problem

We are building roughly a dozen Rust platforms. Each has a Leptos frontend served from its own root, each ships on its own cadence, and each is developed by a team that should not have to coordinate with the other eleven to release.

The cost lands on the people using them. Operational context is spread across a dozen origins, so answering a question that spans two systems means holding two tabs and doing the correlation by hand. There is no place where "how are things" can be answered. Each frontend is coherent on its own; together they are twelve tools rather than one product.

The obvious fixes are both wrong. Building a thirteenth hand-written dashboard puts every cross-cutting view behind a central team's backlog and goes stale the moment a platform changes. Composing the frontends at runtime means a dozen WASM binaries in one document, each with its own allocator and reactive runtime, which is not viable at this count and would not be viable at half it.

## What Hlin is

A composition shell.

Each platform declares, at runtime, what it can show. Hlin discovers those declarations, renders every declared panel through a single design system, and lets a person select panels from any platform and arrange them into a surface they authored.

Nothing crosses the boundary except data and a declared view kind. Hlin does the rendering; platforms do not ship code into it.

That single constraint produces most of the properties we want. Design consistency is structural rather than enforced by review, because every panel is drawn by the same component library. Drag-and-drop composition is tractable, because every panel is the same kind of object to the layout engine. Adding a platform costs the shell nothing, because the shell was never compiled against it.

## What Hlin is not

**Not a reverse proxy.** Path-prefixed routing and single-origin sessions sit underneath Hlin and make cross-platform data fetching and auth tractable. That plumbing is necessary and it is not the product.

**Not micro-frontends.** No runtime composition of independently built WASM modules; not now, not as a later phase. Panels are data, not applications.

**Not a query layer.** Hlin shows what platforms choose to expose. It does not reach into anyone's database, and it defines no query language over platform internals.

**Not a general dashboarding tool.** Grafana exists and is better at being Grafana. Hlin composes views authored by the teams that own the systems, in the vocabulary of those systems.

**Not a central bottleneck.** Shipping a panel does not require a Hlin release, a Hlin PR, or a design review.

## Principles

**The shell renders everything.** Panels cross the boundary as data plus a view kind drawn from a shared vocabulary. A platform names `timeseries`; Hlin decides what a timeseries looks like. Consistency follows from the architecture instead of from discipline.

**Platforms are autonomous.** Adding a platform, a panel, or a navigation entry is a deploy of that platform. Discovery happens at runtime against a manifest each platform serves about itself. Hlin is never rebuilt to accommodate a child.

**The manifest is a public API.** Panels are a declared contract, not an implementation detail. They are versioned, diffed in CI, and deprecated with a window and a named successor. Removing a panel key without a major version is a breaking change; the shell detects it at runtime and flags it, and no build anywhere has to fail for the contract to be enforced.

**Vocabulary is additive and governed.** View kinds are a bounded set owned by the design system. New kinds are added deliberately, with an owner and a bar for admission. Unknown kinds degrade to a fallback; they never break a layout.

**Composition belongs to users.** Selecting and arranging panels requires a deploy from nobody. The set of useful cross-platform views is not knowable in advance by any central team, so we do not try to enumerate it.

**Rendering is total over its inputs.** Every panel is in exactly one state at all times: loading, ready, stale, or unavailable, with unavailability distinguishing unreachable from malformed from unknown from deprecated. There is no state a platform can put a panel into that the shell does not have a rendering for.

## Architecture

### The manifest

Each platform serves a document describing itself at a well-known path. It declares navigation entries, the panels it offers, the parameters each panel responds to, health and summary endpoints, and lifecycle status for anything deprecated.

Three rules keep it survivable across a dozen independent release cadences. The schema version is monotonic and additive; unknown fields are ignored rather than rejected, so nothing requires a coordinated upgrade. Icons and view kinds are names from a shared vocabulary rather than code, so a platform cannot ship rendering logic across the boundary. A malformed or absent manifest degrades to a plain link with a warning indicator; it is never a shell error.

### Discovery

Hlin reads a platform list from runtime configuration and polls each manifest. The registry sits behind a trait, so a push-based model with heartbeat expiry, or orchestrator-native label discovery, can replace configuration later without reshaping anything above it.

### Panels

A panel declaration names a view kind and a data endpoint. The endpoint returns a typed envelope for that kind. Shell-level controls, notably time range, drive every panel that declares the corresponding parameter, which is how eight panels from six platforms respond to one picker without any of them knowing about each other.

Fan-out is aggregated shell-side, deduplicated across panels requesting the same data, and delivered to the browser over a single stream.

### Views

The view registry lives in a companion crate beside the design system, so the component library stays free of Hlin's vocabulary. Adding a view kind is a release of that crate; platforms adopt it on their own schedule by naming it, and never trigger a shell rebuild themselves.

### Customization

Composition-level customization comes first: users choose panels, arrange them, set titles and thresholds and time ranges. No new rendering.

A bounded declarative view spec, where a platform ships a small tree referencing design-system components bound to fields in its own response, is the deliberate escape hatch for genuinely bespoke panels. The data envelope is designed so this drops in without reshaping anything, and it is not built until composition-level customization has demonstrably failed a real case.

Embedding a platform's own frontend in a panel is not a supported mechanism. It breaks design consistency, breaks shared controls, and costs a runtime per instance.

## What we are betting on

That the set of valuable cross-platform views is larger than any central team can enumerate, and that the constraint of a shared rendering vocabulary is a smaller cost than the coordination it removes.

The second half is the part that could be wrong. If platform teams find the vocabulary too narrow to express what their systems actually need to show, they will route around it, and the shell becomes a link farm with extra steps. Guarding against that is what the vocabulary governance principle is for, and it is the thing to watch in the first two adoptions.

## Success condition

A team ships a new panel on Tuesday morning. Someone on another team has it on their dashboard Tuesday afternoon, next to panels from two other platforms, with no Hlin release, no design review, and no conversation between the two teams.

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

### Seeing it in a real design system

The demo pack exists so this repository can demonstrate itself while depending
on nothing published. It is deliberately plain and is not a design system.

`examples/frontend-aurora` is the same front end drawn by Colliery's Aurora
Dark, which is a real one:

```bash
angreal ui build --which frontend-aurora
./target/debug/hlin serve --config demo/hlin-aurora.toml
```

The two binaries differ by one identifier. Choosing a design system is a
dependency and a line; everything else is `hlin-ui`, which has no idea what is
drawing its panels. Nothing in `crates/` mentions Aurora, and Aurora's own
default build does not mention Hlin.

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
