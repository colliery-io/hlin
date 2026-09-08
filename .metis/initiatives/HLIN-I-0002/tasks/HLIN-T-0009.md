---
id: hlin-sample-platform-the-reference
level: task
title: "hlin-sample-platform: the reference platform"
short_code: "HLIN-T-0009"
created_at: 2026-09-07T14:04:17.870682+00:00
updated_at: 2026-09-07T15:38:28.009334+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# hlin-sample-platform: the reference platform

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Build `hlin-sample-platform`: a small axum binary that is what a platform looks like from Hlin's side of the boundary. It serves a manifest and synthetic panel data across the whole envelope vocabulary, and it is the reference every real platform team copies. Run twice with different names, it is the demo's two platforms.

## Acceptance Criteria

## Acceptance Criteria

- [x] New crate `crates/hlin-sample-platform`, an axum binary with `--name`, `--port`, and `--breaking` flags; `--name` is the `platform.id`, and workers, clusters and the options path all carry it so two instances are distinguishable
- [x] Serves `GET /.well-known/hlin.json`; on startup the binary validates its own manifest with `hlin_manifest::validate` and refuses to start if the document or any panel is rejected
- [x] Panels cover the vocabulary: `records-per-second` (`scalar.v1`); `throughput` and `throughput-compact` reading **one shared** `series.v1` endpoint; `queue-depth` (`records.v1`); `worker-health` (`status.v1`); `throughput-by-cluster` with a `select` and an `options.v1` endpoint
- [x] Data endpoints read `from`, `to`, `step` and `cluster`; series carry roughly one point per `step`; the select narrows the data and an unknown selection falls back to the platform's own default; every response passes `parse_envelope` for its declared type
- [x] `--breaking` drops `queue-depth` with `contract_version` unchanged; a test asserts this classifies as `Breaking` with a `Violation` verdict
- [x] A health endpoint at the path the manifest declares
- [x] Data endpoints call one `identify` function, so [[HLIN-T-0010]] replaces its `Token` arm and touches nothing else
- [x] `--auth` selects `session`, `token` or `open`, with `--session-cookie` naming the cookie and `--restrict-health-to` gating one panel by group so a 403 is demoable ([[HLIN-A-0008]], [[HLIN-S-0005]])
- [x] Tests: 14 covering manifest validity, round-trip, vocabulary coverage, the shared endpoint, the select's options path, the breaking diff, envelope acceptance per panel, the `step` hint, gaps, determinism, and two platforms differing

## Implementation Notes

### Technical Approach
Synthetic data should be deterministic per `(name, endpoint, from, to)` so the demo is reproducible and tests are stable: seed from a hash of those. Generate something that looks like a signal rather than noise, so a chart is worth looking at. Keep the crate free of `hlin-view` and `hlin`; a platform depends on `hlin-manifest` and nothing else of Hlin's.

### Dependencies
`hlin-manifest`. Nothing else in the initiative; this can start immediately and in parallel with [[HLIN-T-0010]].

### Risk Considerations
The temptation is to make the sample platform clever. It should be boring and obviously correct, because it is documentation as much as it is code.

## Status Updates

**2026-09-07 — complete.** `hlin-sample-platform` in three modules: `manifest` (the document, built as a value so it can be validated), `data` (synthetic envelopes), `routes` (the HTTP surface). 14 tests; workspace at 109; `angreal check all` clean.

Verified against a running server, not only in tests: the served manifest was fetched with `curl`, parsed and validated by `hlin-manifest` with all six panels accepted; `session` mode answered 401 without a cookie and 200 with one; the restricted panel answered 403 without the group and 200 with it; `token` mode answered 401 without the header.

Decisions taken during the work:

- **`--auth` has three modes rather than two.** `session` and `token` were asked for; `open` was added because a platform team's first five minutes copying this crate should not require understanding identity at all. It warns loudly at startup and is never the default.
- **The demo session cookie carries `principal:group,group`.** A 403 needs a principal that lacks a group, and inventing that needs either an identity provider or a convention. The convention is the cheaper of the two for a demo, and it is confined to the sample platform.
- **The manifest is built as a value, not a string.** That is what makes the startup self-validation possible, and self-validation is the thing that keeps a reference from teaching twelve teams to be wrong.
- **One deliberate gap per series.** A viewer cannot tell whether gaps render correctly unless there is a gap, and `null` drawn as a drop to zero is the classic charting bug this makes visible.
- **Data is deterministic in `(platform, endpoint, window)` but `as_of` is not.** The first test I wrote asserted whole-envelope equality and failed correctly: `as_of` says when the data was true and the answer is now. The test now pins the values, which is the property that matters.

**A bug only running it would have found.** axum refuses `"/api/hlin/{name}-clusters"`: a path parameter cannot be part of a segment. Every test passed and the binary panicked on its first start. The route is now registered as a literal built from the platform's own name, which is better anyway since the platform knows its name at construction. Worth remembering that the tests here exercise the manifest and the data, not the router, and a `curl` found in one second what the suite could not.

**Two clippy lints worth the refactor they forced.** `result_large_err` on returning a whole `Response` as an error led to a small typed `Refusal` with an `IntoResponse` impl, which separates the decision from its rendering and stops a handler pairing a reason with the wrong status. `collapsible_if` tidied the group check.

Note for [[HLIN-T-0010]]: the `Token` arm of `identify` currently accepts any `X-Hlin-Identity` header and warns at startup that it does. Replacing that arm with real verification is the whole of the wiring; no handler changes.
