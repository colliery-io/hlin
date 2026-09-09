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
  - "#phase/completed"

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

## Released — 2026-09-09

`v0.0.1` is out, the repository is public, and every artifact is anonymously
pullable. Verified as an outsider, not asserted:

| Artifact | Where | Checked |
|---|---|---|
| 8 crates | crates.io | all at `0.0.1` |
| image | `ghcr.io/colliery-io/hlin:0.0.1` | linux/amd64 **and** linux/arm64, pulled natively on an arm64 Mac |
| chart | `oci://ghcr.io/colliery-io/charts/hlin` | `helm template` from the registry resolves the image tag that exists |
| binaries | GitHub release | three tarballs, and nothing spurious |

The full trial, run end to end with published artifacts only: the released
image, an anonymous shell, a platform reached with `strategy = "none"`, and a
front end from somebody else's pack image — `serving the frontend from
/home/hlin/frontend`, drawing `frontend-aurora` from an image that ships
`frontend-gallery`.

And an outsider's project — no workspace, no patch — depending on `hlin-ui`,
`hlin-view` and `hlin-pack-demo` from crates.io compiles to wasm.

### What releasing found

Four faults, none of which any amount of rendering or dry-running had shown,
because a release had never been run:

1. **The image was amd64 only.** One build on whichever runner it landed on,
   tagged directly. `docker pull` on an arm64 node or an Apple Silicon laptop
   answered "no matching manifest" — most of the machines somebody would try
   this on. Now built natively on both and joined into one manifest.
2. **The release attached every artifact in the run**, including the
   `.dockerbuild` build record `build-push-action` uploads on its own. It
   refused to extract and failed the job twice; had it succeeded it would have
   been offered as a download.
3. **The crates job asked crates.io without a User-Agent**, which crates.io
   refuses — so its "already published?" check answered no for everything and
   the re-run safety it existed for was never once true. Also, crates.io
   rate-limits new crates: five went through and the sixth came back 429.
4. **Digests as filenames.** `sha256:...` contains a colon, which an artifact
   path may not.

Each is now fixed and the pipeline has run green end to end.
