---
id: serve-a-design-pack-s-own-frontend
level: task
title: "Serve a design pack's own frontend from the shipped image"
short_code: "HLIN-T-0057"
created_at: 2026-09-09T01:14:41.543936+00:00
updated_at: 2026-09-09T01:14:41.543936+00:00
parent: HLIN-I-0009
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0009
---

# Serve a design pack's own frontend from the shipped image

## What

A pack author cannot bring their design pack. The image bakes
`examples/frontend-gallery/dist` at `/home/hlin/frontend` and the chart
hardcodes that path, so serving their own frontend means building their own
image of Hlin from source — which is the thing a trial is supposed to avoid.

## Shape

`config.frontendImage` in the chart: an initContainer copies that image's
`/dist` into an `emptyDir`, mounted over `/home/hlin/frontend`. Unset, nothing
changes and the baked-in gallery is served.

The author's image carries only their built `dist` — a few megabytes on
`busybox`, not a Rust toolchain. The shell image stays ours, which is what
keeps a security update to the shell from being their rebuild.

Document the other way too, for somebody who would rather have one image:
`FROM ghcr.io/colliery-io/hlin` plus `COPY dist /home/hlin/frontend`.

## Done when

- `helm template` renders the initContainer, the volume and the mount when
  `frontendImage` is set, and none of them when it is not.
- A frontend built here, put in a scratch image, is actually served by the
  shipped Hlin image — proven by running it, not by rendering YAML.
- README says how, in the terms a pack author thinks in.

## Status Updates

- 2026-09-09: Done. `config.frontendImage` (with `frontendImagePullPolicy` and
  `frontendImagePath`): an init container copies that image's `dist` into an
  emptyDir mounted over `/home/hlin/frontend`. Unset, nothing renders and the
  bundled front end is served — checked both ways.
- The init container refuses an empty copy. An image with nothing at that path
  would otherwise leave the shell serving no front end at all, which looks like
  the shell being broken and is not.
- Proven by running it, not by rendering YAML:
  - A pack author's image is `FROM busybox` + `COPY dist /dist` — **14MB**, no
    Rust toolchain.
  - The copy command, run exactly as the chart runs it, produced index.html,
    the js and the wasm; against an image with nothing there it exited 1 with
    the message.
  - The **shipped image, built with `frontend-gallery`, served
    `frontend-aurora`** from the mounted volume — and the wasm it handed over
    was byte-identical to the one in the pack image (`37beb8fbce61ee91`). The
    log line reads `serving the frontend from /home/hlin/frontend`.
