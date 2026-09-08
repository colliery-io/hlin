---
id: shell-host-manifest-client-and
level: task
title: "Shell host, manifest client and registry loop"
short_code: "HLIN-T-0011"
created_at: 2026-09-07T14:04:20.862499+00:00
updated_at: 2026-09-07T16:06:29.335758+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Shell host, manifest client and registry loop

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Make the shell a running service that knows what platforms exist and what they promise. `hlin serve` starts an axum server, reads a configuration naming the platforms, polls each manifest, validates and diffs it, remembers it through the store, and says what it found. This is the registry from the system decomposition, and the first place the runtime-enforcement model of [[HLIN-A-0002]] actually runs.

## Acceptance Criteria

## Acceptance Criteria

- [x] `hlin serve` and `hlin check`, axum on a configured port, a TOML config covering platforms, timings, database, key path and development principal, with `demo/hlin.toml` as the worked example
- [x] A `reqwest` manifest client fetching the well-known path with the shell's own identity, classifying the outcome as `Document`, `Unreachable` or `Unreadable`
- [x] A `Registry` with a poll loop that validates against the configured id, records via `Store::observe_platform`, and classifies once the observation count crosses the debounce threshold
- [x] Verdicts logged at the right level with specifics: violation as an error naming the platform, the breaking changes and the expected major; rollback as info; accepted as debug; a newer schema version as info
- [x] `restore()` loads every stored snapshot at startup, and a test proves a change shipped across a restart is judged on the first poll after it
- [x] A `PlatformView` per platform with manifest, validation, accepted panels, reachability and trouble, read by `views()`
- [x] `GET /api/platforms` with reachability, contract version and hash, accepted panels including the kinds each envelope allows, and rejected panels with reasons. Shaped once here as the picker's source for [[HLIN-T-0015]]
- [x] `GET /.well-known/hlin-keys.json` serving the issuer's JWKS, from a key loaded or generated at the configured path
- [x] The `Credentialer` trait with `hlin-token`, `forward-session` and `static-bearer`, selected per platform. A `Viewer` carries what a person brought; it is never logged or stored. The `dev` authenticator is `Config::principal()` rather than a trait, since one strategy does not need a trait yet
- [x] Startup rules enforced with the platform named: `forward-session` refused across an origin boundary with `hlin-token` offered as the alternative; `static-bearer` refused without acknowledgement and warned about at startup when used
- [x] A 15-test conformance suite over every credentialer, plus the configuration rules
- [x] 9 registry tests against `MemoryStore` and a scripted client, covering discovery, an unreachable platform keeping its panels, a malformed manifest, accumulation, debounce, flapping, restart, additive change and rollback

## Implementation Notes

### Technical Approach
The manifest client and the registry are separate traits so each is testable alone: the registry's tests never touch the network, and the client's never touch the store. Keep the poll loop one task per platform on a `tokio` interval so one slow platform cannot hold up the others. The `/api/platforms` shape is the picker's data source in [[HLIN-T-0015]]; design it once.

### Dependencies
[[HLIN-T-0009]] for a real platform to point at; the store from [[HLIN-T-0007]]; the `Issuer` from [[HLIN-T-0010]] for the JWKS route.

### Risk Considerations
Debounce interacts with the store's observation count in a way that is easy to get off by one; the tests should pin the exact poll on which a violation is raised.

## Status Updates

**2026-09-07 — complete.** The shell runs. Five new modules in `hlin`: `config`, `identity` (the credentialer strategies), `manifest_client`, `registry`, `server`. 24 tests here; workspace at 172; `angreal check all` clean.

Verified against two running sample platforms, not only in tests: `hlin serve` discovers both, `GET /api/platforms` reports twelve panels with their available kinds, the JWKS route serves the shell's key, and restarting a platform with `--breaking` produces the violation line naming the platform, the removed panel and the expected major version.

**A real bug that only running it found, and the one the task's risk note predicted.** Debounce worked, and the violation never fired. The cause: the in-memory manifest was updated on every successful poll, including polls where the change had not yet been believed. So the second sighting of a change compared against the first sighting rather than against the last judged contract, found no difference, and silently accepted it. The fix separates two things that were conflated: `manifest` is what the platform is serving now and moves every poll, because a viewer should see current panels; `classified` is the baseline a change is judged against and moves only when a change is actually judged. `a_breaking_change_is_judged_only_once_it_has_held` now pins both halves, asserting that panels update at the first sighting while the contract hash does not.

Decisions taken during the work:

- **The `dev` authenticator is a method, not a trait.** One strategy does not need a trait; `Config::principal()` is honest about that, and `oidc` or `trusted-header` will introduce the trait when there is a second.
- **The same-origin check for `forward-session` is strict about ports.** The demo runs platforms on their own ports, which is not same-origin with the shell, so the demo config uses `hlin-token` for both and the specification's day-one story is demonstrated by the sample platform's `--auth session` mode rather than by the shell forwarding a cookie across ports. That is the check working, not a gap.
- **Running without a database is allowed and warned about loudly.** The warning says what is actually lost: contract memory does not survive a restart, so a breaking change shipped across one goes unnoticed.
- **The poll loop is one task over all platforms** rather than one per platform. The fetch has a timeout so a slow platform cannot hold up the others for long, and during a demo the log stays readable. One task per platform is a small change when it matters.
- **An unreachable platform keeps its panels.** A surface a person is looking at should degrade rather than empty out, so the last contract stays in place while a platform is away.

Note for [[HLIN-T-0012]]: `Registry::views()` gives the aggregator the accepted panels and each platform's `Credentialer`; asking that for headers is the whole of the identity work on the data path.
