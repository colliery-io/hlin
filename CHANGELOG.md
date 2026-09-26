# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] — 2026-09-26

Hlin is now the place people work across many systems, not only a vantage over
them. Platforms ship their own UI as modules; the shell runs each one in a
sandboxed frame and owns the page, the person and the wire (HLIN-A-0014). The
shell can still draw a panel from data itself, and does when a module cannot
load.

### Breaking

- `hlin-manifest`: `Panel.kind`, `envelope` and `data` are optional, because a
  panel may be drawn only by its platform's module. Code that builds a `Panel`
  by hand must say which it is; `Panel::drawn_by_shell()` returns all three
  when all are present.
- The shell's default origin, with no `public_url`, is the one a request
  arrived on rather than `http://localhost:{port}`. A shell reachable from
  other machines should set `public_url`.

### Modules

- A manifest may declare `ui` on a panel or a navigation entry, an `assets`
  prefix, and `routes` for reads and writes (HLIN-S-0001).
- The shell serves a module's files from its own origin under `/m/`, each
  under a CSP that lets it reach nothing but its own assets, compressed once
  and kept, and never follows a platform's redirect.
- Modules run in sandboxed frames with an opaque origin and talk to the page
  over a versioned `postMessage` bridge (HLIN-S-0007): context, theme,
  requests, parameters, navigation, notices, and changes both ways.
- A module's requests reach its own platform through `/p/`, under the
  prefixes it declared, as the viewer, with writes carrying a token bound to
  the method and path (`htm`, `htu`) and an idempotency key, never retried
  (HLIN-A-0013).
- Streamed reads, with credit so nothing buffers without bound.
- A frame budget of twelve per surface, with `suspend` and `state` so a module
  scrolled away keeps what it held.
- Platform pages open inside the shell at `/page/{platform}/…`.
- A platform that stops answering dims its panels after three of the shell's
  own refusals, and they recover by themselves when it returns.
- New crates: `hlin-bridge` (the messages) and `hlin-module` (the SDK a
  Leptos module is built with). The SDK warns in debug builds when a module
  holds the main thread.

### Identity

- `oidc` sessions carry the person's email to platforms in the token.
- A debug build may sign in against a local identity provider over plain
  http; a release build still requires https.
- The event stream signs every resubscribe afresh, so a platform restarted
  after its token has expired is followed again.

### The shell

- Time controls appear only on a surface with a panel that wants them.
- Limits for modules are configuration (`[modules.limits]`), per platform.

### Samples and demos

- `hlin-sample-checklist` and `hlin-sample-feed`: platforms people change,
  each with its own rules, its own module, and its own words for a refusal.
- `angreal demo up --with collab`: sign in through Dex as Alice, Bob or Carol
  and see each other's changes without a reload.
- `angreal demo up --with twenty`, and `--with twenty-compose`: twenty
  platforms on one surface, each serving its own UI at `/` and Hlin under
  `/hlin`, each module a re-export of its UI's components; the second as
  twenty containers with a release shell and Dex.

### Known

- A module that holds its main thread can freeze the page outside full
  Chromium, where frames share the page's process. Accepted for this release
  and documented (HLIN-S-0007).

## [0.0.1] — 2026-09-08

First alpha. Everything below is new, so this is a description of what the
release *is* rather than of what changed in it.

Alpha means the shell runs, the contracts are enforced, and the shape is
settled enough to build against — not that any of it is stable. Every version
number here can move, including the ones a platform declares.

### The shell

- Reads a manifest from each configured platform at `.well-known/hlin.json`,
  offers its panels, and serves a surface over server-sent events.
- Enforces the contract at runtime: a content hash plus semver, so a platform
  that ships a breaking change without a major bump is caught and recorded
  rather than silently believed (HLIN-A-0002).
- Degrades per panel. A platform that is unreachable, refuses a viewer, answers
  with something unusable, or withdraws a panel costs that panel and nothing
  else.
- Deduplicates upstream requests per principal, so ten people watching one
  wallboard cost what one person costs.
- Remembers contracts and layouts in Postgres, so a breaking change shipped
  across a restart is still caught.

### Composition

- Surfaces are composed by the people who use them: pick panels from any
  platform, arrange them, and the arrangement is theirs.
- Layouts are personal or published, and sharing is read-only with fork to
  edit (HLIN-A-0007).
- One time picker drives every panel on a surface.

### Views and design systems

- Six view kinds and five envelopes, with an acceptance matrix: a panel
  declares what its data *is*, and a viewer may draw it any way that accepts
  that shape (HLIN-A-0003).
- The shell owns no design system. A front end is built by choosing a design
  pack and mounting `hlin-ui`; three are included, drawn by different packs.
- A platform may name a component its design system has and the vocabulary does
  not. A pack that does not recognise it draws the declared kind instead, so
  naming one can never make a panel undrawable.
- Components can talk back, within a closed vocabulary: a click can set a
  parameter or move the whole surface's time range, and nothing else.

### Identity

- Four authenticator strategies: `anonymous`, `trusted-header`, `oidc` with
  sessions in Postgres, and `dev` (refused in release builds).
- `anonymous`, so an identity provider is not required to run Hlin
  (HLIN-A-0012). Nobody signs in; every visitor gets their own identity from a
  cookie the shell sets, because a surface is keyed by its viewer and one
  shared anonymous principal would have a single click move everybody else's
  charts. A shell using it refuses every write, and the front end stops
  offering Edit rather than failing at Save — that refusal is what makes
  anonymity safe rather than merely convenient, and it is not a separate
  setting, because `anonymous` plus writes is `dev`. An open instance shows
  what has been published; compose against the same database from a shell with
  an authenticator.
- Four credentialer strategies for what the shell sends a platform:
  `forward-session`, `hlin-token`, `static-bearer`, and `none` for a platform
  that is open and asks for nothing.
- Authentication is an extractor, so a handler that needs a principal says so
  in its signature and cannot be written without one — and a handler that
  *changes* something asks for a second one, so the read-only refusal cannot be
  forgotten on a route added later.

### Freshness

- A panel declares how often its data is worth refetching; the shell clamps it
  (HLIN-A-0009).
- A platform may declare an event stream and report when a panel's data
  changed. The shell subscribes, once per platform, and keeps polling
  underneath at a relaxed interval — so a stream that is absent, broken, or
  lying cannot make the shell worse than polling alone (HLIN-A-0011).

### Packaging

- A container image at `ghcr.io/colliery-io/hlin`, carrying the shell and a
  front end together. This is the artefact to deploy.
- A Helm chart at `oci://ghcr.io/colliery-io/charts/hlin`, which brings a
  Postgres by default so that `helm install` produces something that can do
  what Hlin is for. It refuses rather than renders what would fail quietly:
  `dev` auth, an unacknowledged `trusted-header`, replicas without a shared
  signing key, no database at all, `oidc` without a public URL or a client
  secret, and two trust bundles where only one can be mounted. Each mirrors a
  refusal the shell makes, for the reason the shell makes it.
- `database_url_env`, so a connection string can come from the environment
  rather than from a file that then has to be treated as a secret. A named
  variable that is absent is refused at startup rather than leaving the shell
  running with no database, which looks identical to not having configured one.
- `bind`, so the shell can listen somewhere other than loopback. It could not
  before, which made it unreachable in a container.
- `ca_bundle`, naming certificate authorities to trust on top of the host's
  own. Merged with the host store rather than replacing it, so a deployment
  with platforms on an internal CA and an identity provider on a public one
  needs no choice between them. It reaches every outbound connection — every
  platform, every stream, and the identity provider — because the clients are
  now built in one place rather than four. A bundle that cannot be read, or
  that holds no certificates, is refused at startup: trusting nothing extra
  fails later against every platform at once and looks like an outage.
  `config.caBundle` in the chart mounts one from a value, a ConfigMap or a
  Secret.
- `config.frontendImage`, so a deployment draws its panels with its own design
  pack without rebuilding Hlin. An init container copies that image's `dist`
  over the front end the shell image was built with, which keeps the shell
  image upstream's — a fix to it is a tag bump rather than your rebuild.
- The examples name Hlin's crates by version, from crates.io, the way anybody
  outside this repository writes them. A `[patch.crates-io]` table resolves
  them to the source next door while working here, so a change to `hlin-ui`
  still reaches the demo and the browser suite.
- The crates are published to crates.io, in dependency order, by the release
  workflow. Four inter-crate dependencies carried a path with no version, which
  made every crate above them unpublishable; nothing had ever run
  `cargo publish --dry-run` to find out.
- Binaries for linux-x86_64, linux-aarch64 and darwin-aarch64, attached to the
  release.

### Unreleased

- **`hlin-ui` puts its own stylesheet on the page.** It shipped in the crate but
  nothing could reach it: the examples here linked it by a relative path into
  the source tree, and a consumer has no path into their registry cache. A front
  end built by following the README rendered every panel unstyled and stacked in
  a column, with nothing to say what was wrong. `APP_CSS` is now public and
  `App` injects it. Found by building the documented front end from scratch and
  deploying it.
- **`hlin-sample-platform` takes `--bind`.** It listened on loopback with no way
  to change that, so it could not be containerised at all — it started, logged
  that it was listening, looked healthy, and was reachable by nothing. The same
  defect the shell had before `bind`.

- **No vendored source and no path dependencies outside the workspace.**
  Aurora's `hlin` feature is upstream and released as `colliery-io-aurora`
  0.2.0, so `vendor/aurora-leptos` is gone and the examples take it from
  crates.io. The feature is additive and off by default: Aurora's own default
  build compiles no Hlin dependency and does not know Hlin exists.

- **Breaking, chart only.** The shell's pods and Service now carry
  `app.kubernetes.io/component: shell`, which the Postgres half has had from
  the start. Without it the shell's Service selected on `name` alone and
  matched the database too: routing survived only because a Service drops a
  pod with no matching named port, and everything resolving the selector
  directly did not — `kubectl port-forward svc/hlin` opened a tunnel to
  Postgres. A Deployment's selector is immutable, so an existing release must
  be uninstalled rather than upgraded. Found by installing the chart into a
  real cluster, which nothing had done: `helm template` cannot see it.

### Fixed by the first release

- **The image was amd64 only.** A single build on the runner that happened to
  be amd64, tagged directly, so `docker pull` on an arm64 node or an Apple
  Silicon laptop answered "no matching manifest". Now built natively on both
  architectures and joined into one manifest — emulation would have made a
  build that compiles Rust twice, once native and once to wasm, take hours.
- The release job attached every artifact in the run, which included the
  `.dockerbuild` build record `build-push-action` uploads on its own. It
  refused to extract and failed the job; had it succeeded it would have been
  offered as a download.
- The crates job asked crates.io whether a version was already published
  without a User-Agent, which crates.io refuses — so it answered "no" for
  everything and the re-run safety it existed for was never once true.
  crates.io also rate-limits new crates, which a workspace this size hits on
  its first release: five went through and the sixth came back 429.

### Fixed before anybody hit them

- The chart asked for an image tag the workflow never pushed. The chart job
  strips the `v` from the git tag and the image job did not, while `image.tag`
  defaults to the chart's appVersion — so the first `helm install` anybody ran
  would have been an ImagePullBackOff. `angreal version verify` now checks that
  the tag the chart resolves is one the workflow pushes, and also tracks
  `Chart.yaml`, which no version command had ever touched.
- A default install's notes claimed no database was configured while the chart
  was busy provisioning one, and said the shell would "run and warn" when a
  release build refuses.

### Known limitations

- **The release tarballs are the binary alone.** One serves the API and has
  nothing to look at; building a front end needs the repository, `trunk` and
  the wasm toolchain. Use the image.
- Postgres is required, and a release build refuses to start without it. The
  in-memory store is a test double (HLIN-A-0006), and falling back to it lost
  every view anybody composed on the next restart — the success condition
  failing quietly, which is worse than failing to start.
- Nothing is published to crates.io.
