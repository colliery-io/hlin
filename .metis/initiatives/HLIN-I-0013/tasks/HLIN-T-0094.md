---
id: twenty-widget-containers-a-shell
level: task
title: "Twenty widget containers, a shell and Dex in one compose file"
short_code: "HLIN-T-0094"
created_at: 2026-09-25T23:54:59.675828+00:00
updated_at: 2026-09-26T02:34:35.000000+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: true
initiative_id: HLIN-I-0013
---

# Twenty widget containers, a shell and Dex in one compose file

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Bring up the intended deployment on one machine: twenty widget containers
each on its own `*.comp.test` name, the shell as a release build on `oidc`
against Dex, and Postgres, all on one compose network with only the shell's
port published.

## Acceptance Criteria

## Acceptance Criteria

- [x] A Dockerfile whose builder stage compiles the shell, its frontend, every
      widget binary, every module and the core UI once (release, `wasm-opt`),
      and whose final stages give one small image per widget and one for the
      shell; BuildKit caching so an unchanged rebuild is quick
- [x] A compose file (its own, or a profile in `docker-compose.yml`) with
      Postgres, Dex, `hlin.comp.test` and twenty widgets as
      `<name>.comp.test`, each serving its core UI at `/` and Hlin under
      `/hlin`, healthchecks, and only the shell published
- [x] The shell's config lists the twenty with
      `base_url = "http://<name>.comp.test:8080/hlin"` on `hlin-token`, and
      `public_url` for the published address; Dex's static client matches
- [x] An angreal way to bring it up and down (e.g. `demo up --with
      twenty-compose`), which publishes "Twenty" by signing in through Dex as
      the collab flavour does, and prints how to open it
- [x] A widget can be stopped and started (`docker compose stop clock`) as a
      platform going down and coming back
- [x] Build time cold and warm recorded; image sizes recorded
- [x] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0093]], [[HLIN-T-0096]] and [[HLIN-T-0097]]. The repository's existing `Dockerfile` builds
  the shell image with a frontend; build on it rather than beside it.
- Dex and its config already exist for the collab flavour.

## Status Updates

### 2026-09-25

Created. Not started.

### 2026-09-26 — the twenty in containers

- **Images.** The repository `Dockerfile` keeps its shell image as the last
  stage and gains one builder for everything: `docker/build.sh` compiles
  every WebAssembly crate in one `cargo build --release --target wasm32`,
  runs a `trunk build --release` per crate in parallel (wasm-opt from
  binaryen `version_123`, installed in the image rather than fetched by
  Trunk), then `cargo build --release` of the shell and every widget with
  `--features <widget>/embed`. Registry and `target/` are BuildKit cache
  mounts; binaries and front ends are copied out to `/out`. `ARG TWENTY`
  (default 0) decides whether the widgets are built, so the release
  workflow's shell image does not wait on twenty servers it does not ship.
  Runtimes: a shared `base` (debian-slim, ca-certificates, curl, user
  `hlin`; curl because a widget fetches the shell's keys with it and every
  healthcheck is one request), `widget` (`ARG WIDGET`, one binary), and the
  shell (`runtime`), which now also carries `frontend-demo`.
- **Compose.** `deploy/twenty/compose.yml`, project `hlin-twenty`, its own
  network (`comp`), Postgres and Dex. Twenty widgets as
  `<name>.comp.test`, `--port 8080 --bind 0.0.0.0`, healthcheck on
  `/hlin/.well-known/hlin.json`, `restart: unless-stopped` (a container
  stopped by hand stays stopped; `start` brings it back). The shell as
  `hlin.comp.test`, release, `oidc`, published on `127.0.0.1:8090`;
  `deploy/twenty/hlin.toml` lists the twenty at
  `http://<name>.comp.test:8080/hlin`. `up` refuses to start if that file
  or the compose file has drifted from `WIDGETS`.
- **Dex and the issuer.** A release shell refuses any plain-http issuer, so
  Dex serves https from a throwaway CA `up` makes with openssl
  (`demo/state/twenty/tls`); the shell trusts it through `ca_bundle`. The
  issuer is `https://dex.localhost:5557/dex`: the browser (and macOS's
  resolver, and curl) resolves `*.localhost` to loopback, where Dex is
  published on `127.0.0.1:5557`; the shell's container resolves the same
  name on the compose network, where it is Dex's alias, and Dex listens on
  5557 inside too. One string, same Dex, from both sides, with no hosts
  file edit and no forwarder. The cost: a browser warns once about the CA
  (Playwright: `ignoreHTTPSErrors`). **So Dex's port is published as well
  as the shell's** (loopback only), which the browser's sign-in requires;
  the widgets and Postgres are not published.
- **Local user: on**, deliberately (`--local-user=Local User`), so each
  widget's own UI at `/` works; nothing publishes a widget, so only the
  compose network reaches `/api/`.
- **Secret.** `demo/state/twenty/oidc.env`, made once, read by Dex and the
  shell through `env_file`, so a plain `docker compose up/stop/start` keeps
  them agreeing.
- **angreal.** `demo up --with twenty-compose` builds, `up -d --wait`s for
  every healthcheck, waits for Dex's discovery (with the CA) and the shell,
  signs in through Dex as Alice (the collab seeding, now taking a shell URL
  and a TLS context) and publishes "Twenty", then prints the URL, the
  people, and the stop/start commands. `demo down` runs `compose down
  --volumes`. Neither touches the process demo, 8080 or the dev database.
- **Build times** (12-core arm64 Docker Desktop, 32 GB): cold, `--no-cache`,
  **10 min 48 s** (toolchain layers 1.5 min; WebAssembly compile 71 s, 42
  parallel Trunk + wasm-opt runs 5 min; twenty-one release servers 2 min
  42 s). Warm, unchanged: **2.2 s** for all twenty-one images. Warm after a
  one-line edit to a widget's server: **1 min 55 s**, because every Trunk
  build reruns and so every embedding widget relinks.
- **Image sizes.** Each widget 164–167 MB on disk, of which 157.9 MB is the
  shared base and 6–9 MB its own (the binary with both builds embedded,
  5–7 MB); 36 MB compressed. The shell 180 MB (22 MB its own: the 10.8 MB
  binary, 2.2 MB gallery, 1.9 MB demo front end). All twenty-one together:
  about 158 MB shared plus about 170 MB unique.
- **Verified by hand.** `demo up --with twenty-compose` came up healthy and
  published "Twenty". Playwright, signed in through Dex's form at
  `https://dex.localhost:5557` as Alice and as Bob: 20 of 20 panels
  `ready` and drawn; a counter bump by Alice reached Bob in 111 ms. From
  inside the network, `http://clock.comp.test:8080/` is the clock's own UI
  (`<title>Clocks</title>`), `/nope` 200 (its UI), `/hlin/nope` 404.
  `docker compose ... stop clock`: the clock panel fell back to "this
  platform is not responding"; `start clock`: healthy again and the panel
  `ready` with its clocks. `demo down --keep-database` removed every
  container and volume of the project.
- `angreal check all` clean; `angreal test all` 1085 passed, 3 ignored.
