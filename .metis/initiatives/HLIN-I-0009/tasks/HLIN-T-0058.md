---
id: publish-the-crates-a-pack-author
level: task
title: "Publish the crates a pack author needs"
short_code: "HLIN-T-0058"
created_at: 2026-09-09T01:14:44.157739+00:00
updated_at: 2026-09-09T01:14:44.157739+00:00
parent: HLIN-I-0009
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0009
---

# Publish the crates a pack author needs

## What

Publish to crates.io. Without it a pack cannot be written at all: `hlin-view`
carries the `DesignPack` trait and `hlin-ui` the app that mounts it.

## Found already

Four inter-crate dependencies carried a path with no version, so every crate
above them refused to publish — `all dependencies must have a version
requirement specified`. Invisible until the first `cargo publish --dry-run`.
Fixed in the workspace dependency table.

After that, every remaining dry-run failure is `no matching package named ...
found`, which is only the chicken-and-egg of a first publish and resolves by
publishing in dependency order:

```
hlin-manifest, hlin-identity     (no internal dependencies)
hlin-view, hlin-stream
hlin-ui, hlin-pack-demo
hlin-sample-platform, hlin
```

## Shape

- A `crates` job on the release workflow, publishing in that order, gated on a
  `CARGO_REGISTRY_TOKEN` secret.
- Publishing is irreversible per version, so the order must be encoded, not
  remembered.
- `hlin-ui` has no README and is a front door for this audience. So is
  `hlin-view`, which has one.

## Done when

- `cargo publish --dry-run` passes for every crate that has no unpublished
  internal dependency, and the rest fail only on the registry-order reason.
- The workflow publishes in dependency order.
- The front-door crates read like something a stranger can start from.

## Status Updates

- 2026-09-09: Done. A `crates` job on the release workflow publishes in
  dependency order, skips a version already on crates.io so a re-run finishes a
  half-finished tag, and fails loudly on a missing `CARGO_REGISTRY_TOKEN`.
- Deliberately *not* a dependency of the `release` job: a missing or expired
  crates.io token should not swallow the binaries, the image and the chart with
  it.
- After adding the four missing versions, a dry-run sweep leaves exactly one
  failure class — `no matching package named …`, the chicken-and-egg of a first
  publish, which the job's ordering resolves. `hlin-manifest` and
  `hlin-identity` publish today; the other six wait only on those.
- `hlin-ui` gained a README and a description worth reading: it is the front
  door for this audience and had neither.
