# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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

- Three authenticator strategies: `dev` (refused in release builds),
  `trusted-header`, and `oidc` with sessions in Postgres.
- Three credentialer strategies for what the shell sends a platform:
  `forward-session`, `hlin-token`, and `static-bearer`.
- Authentication is an extractor, so a handler that needs a principal says so
  in its signature and cannot be written without one.

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
- Binaries for linux-x86_64, linux-aarch64 and darwin-aarch64, attached to the
  release.

### Known limitations

- **The release tarballs are the binary alone.** One serves the API and has
  nothing to look at; building a front end needs the repository, `trunk` and
  the wasm toolchain. Use the image.
- Postgres is required for anything to survive a restart. Without it the shell
  runs and warns.
- `oidc` has not been run against a real provider.
- Nothing is published to crates.io.
