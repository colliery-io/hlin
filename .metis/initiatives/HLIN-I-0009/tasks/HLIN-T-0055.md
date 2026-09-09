---
id: the-chart-asks-for-an-image-tag-ci
level: task
title: "The chart asks for an image tag CI actually pushes"
short_code: "HLIN-T-0055"
created_at: 2026-09-09T01:14:36.670047+00:00
updated_at: 2026-09-09T01:14:36.670047+00:00
parent: HLIN-I-0009
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0009
---

# The chart asks for an image tag CI actually pushes

## What

The chart asks for an image tag the release workflow never pushes.

| | |
|---|---|
| chart job | `version="${GITHUB_REF_NAME#v}"` → chart appVersion `0.0.1` |
| image job | `tags: ...:${{ github.ref_name }}` → pushes `:v0.0.1`, `:latest` |

`values.yaml` leaves `image.tag` empty, which defaults to `.Chart.AppVersion`,
so a first `helm install` resolves `ghcr.io/colliery-io/hlin:0.0.1` — never
pushed. ImagePullBackOff, on the very first thing anybody does.

## Shape

Strip the `v` in the image job the way the chart job already does, and keep the
`v`-prefixed tag too so a person who read the git tag finds what they expect.

## Done when

- The workflow pushes the tag the chart's appVersion names.
- Something asserts the two agree, so this cannot drift back silently. It is a
  cross-file invariant between YAML and YAML, which nothing currently checks.

## Status Updates

- 2026-09-09: Done. The image job now pushes the bare version alongside the
  `v`-prefixed tag and `latest`, so the tag the chart's appVersion resolves is
  one that exists.
- Guarded rather than just fixed: `angreal version verify` reads the tag list
  out of the workflow, substitutes both expressions the workflow uses to spell
  a version, and asserts the chart's appVersion is among them. Proven by
  reintroducing the exact bug — exit 1, with "chart resolves this image tag;
  the workflow pushes v0.0.1, latest".
- It also now tracks `charts/hlin/Chart.yaml`, which no version command had
  ever touched, so `version bump` left the chart behind and `version verify`
  called that in sync.
- The runner drops buffered stdout when a task exits non-zero, so the first
  version of this check reported an exit code and no reason. Flushed.
