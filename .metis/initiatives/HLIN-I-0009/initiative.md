---
id: a-platform-author-can-trial-hlin
level: initiative
title: "A platform author can trial Hlin"
short_code: "HLIN-I-0009"
created_at: 2026-09-09T01:10:00.000000+00:00
updated_at: 2026-09-09T01:10:00.000000+00:00
parent: hlin
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/ready"

exit_criteria_met: false
estimated_complexity: M
initiative_id: a-platform-author-can-trial-hlin
---

# A platform author can trial Hlin

## What a trial is

Deploy Hlin from the Helm chart, point it at their own platform, and draw it
with their own Leptos design pack. Not a canned demo — the real thing, against
their own work. That is the audience: somebody who has a Leptos platform and
wants to see it inside a shell.

## What stops them today

Measured, not assumed — `helm template`, `cargo publish --dry-run`, and reading
the workflow:

1. **Nothing is published.** No `v*` tag has run, so neither the image nor the
   chart exists. Expected.
2. **The repo is private**, so packages a workflow publishes inherit that.
3. **The chart asks for an image tag CI never pushes.** The chart job strips the
   `v` (`${GITHUB_REF_NAME#v}` → appVersion `0.0.1`); the image job does not
   (`:${{ github.ref_name }}` → `:v0.0.1`). `image.tag` defaults to appVersion,
   so a first `helm install` is an ImagePullBackOff.
4. **A design pack cannot be brought.** The image bakes
   `examples/frontend-gallery/dist` at `/home/hlin/frontend`, and the chart
   hardcodes that path. A pack author has no way to serve their own frontend
   short of building their own image from scratch.
5. **The crates are not on crates.io**, so a pack cannot be written at all:
   `hlin-view` carries the `DesignPack` trait and `hlin-ui` the app that mounts
   it. Four inter-crate dependencies also carried a path with no version, which
   made every crate above them unpublishable — invisible until the first
   `cargo publish --dry-run`.
6. **The chart lies to a default install**, telling it "No database configured.
   The shell will run and warn" while the bundled Postgres is on and a release
   build refuses rather than warns.

## Success condition

Somebody outside this repository can: depend on `hlin-view` and `hlin-ui` from
crates.io, write a pack, build a frontend, `helm install` Hlin, have it serve
their frontend and poll their platform — without cloning this repository or
building Hlin from source.

## Out of scope

- Making the repository or its packages public. That is the owner's call and a
  one-line setting, not work.
- Tagging `v0.0.1`.
- Bundling sample platforms in the chart. The audience brings their own
  platform; that is the premise.

## Status Updates

- 2026-09-09: Decomposed. Four tasks, T-0055..T-0058.
- 2026-09-09: All four complete. What remains between here and a stranger
  trialling Hlin is not code: tag `v0.0.1`, set `CARGO_REGISTRY_TOKEN`, and
  decide whether the packages are public.
- The two blockers that were bugs are fixed and guarded. The image-tag
  mismatch would have made the first `helm install` anybody ran an
  ImagePullBackOff; `angreal version verify` now fails on it, proven by
  reintroducing it.
- Awaiting review. Not transitioned to completed.
