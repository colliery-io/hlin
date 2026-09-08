---
id: ready-the-contract-crates-for
level: task
title: "Ready the contract crates for publication"
short_code: "HLIN-T-0022"
created_at: 2026-09-07T22:23:00+00:00
updated_at: 2026-09-07T23:42:15.260226+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# Ready the contract crates for publication

## Parent Initiative

[[HLIN-I-0003]]

## Objective

A design system that implements `DesignPack` takes a dependency on
`hlin-view` and, through it, `hlin-manifest`. Make that a dependency somebody
would accept: small, well named, and carrying nothing it does not need.

Nothing is released here. This is the preparation, done while the interface can
still change freely.

## Acceptance Criteria

## Acceptance Criteria


- [x] `hlin-manifest`'s fingerprinting dependencies behind a feature, so a pack pulling the envelope types does not compile `sha2`, `semver` or `serde_jcs`
- [x] The shell and the registry keep those on, and the contract tests still cover the fingerprint
- [~] Published names **recorded rather than settled**, and put to a person in the status update. `hlin-manifest`, `hlin-view` and `hlin` are all free on crates.io, and `hlin` is not a generic name in the way `aurora` is, so the org prefix may not be needed. Not renamed, because that costs churn and buys nothing before publication
- [x] `cargo package --list` clean for both crates: no stray files, the licence present, metadata complete
- [~] `cargo publish --dry-run` passes for `hlin-manifest`. It **cannot** pass for `hlin-view` until `hlin-manifest` is actually on the registry, because the dry run resolves dependencies against crates.io. That is the release order, not a defect
- [x] Nothing published

## Implementation Notes

### Technical Approach
The split is between the envelope types, which a pack needs, and contract
identity, which only the shell computes. `Envelope`, its five shapes and the
validation belong to everyone; `contract_hash`, the canonicaliser and the semver
diff belong to the shell. A default-off feature is the wrong way round: leave
them on by default so the shell and every existing test are unaffected, and let
a pack ask for `default-features = false`.

Names to settle rather than guess at: `colliery-io-hlin-view` and
`colliery-io-hlin-manifest` on the registry, imported as `hlin_view` and
`hlin_manifest`, which is exactly what `colliery-io-aurora` does today.

### Dependencies
None strictly, but doing it after [[HLIN-T-0019]] and [[HLIN-T-0020]] means the
interface being readied is the final one rather than a version of it.

### Risk Considerations
Feature-gating a crate's internals usually breaks somebody's build in a way
`cargo test` does not catch, because the workspace always has every feature on.
Build the pack path with `default-features = false` explicitly, or the gate is
decorative.

## Status Updates

### 2026-09-07 — ready, not published, and two decisions to put to a person

**The gate is real, and verified as real.** `hlin-manifest` splits along a clean
line nobody had to invent: `envelope` and `envelope_validate` depend on nothing
but serde, chrono and thiserror, while `canonical`, `diff`, `manifest`,
`params`, `path`, `validate` and `errors` are the contract half. The `contract`
feature is on by default so the shell and every existing test are untouched, and
`hlin-view` takes the crate without it.

The risk note warned the gate would be decorative because the workspace always
has every feature on. It is not:

```
cargo tree -p hlin-view -e normal | grep -E "sha2|semver|serde_jcs"
→ nothing
```

A design system implementing `DesignPack` now pulls serde, serde_json, chrono
and thiserror, and compiles no hash function to draw a chart.

**Cargo would not let the dependency be inherited.** A workspace dependency
cannot have `default-features` overridden at the use site, so `hlin-view`
declares `hlin-manifest` directly with both a path and a version. That is the
publishable form anyway: the path resolves in this workspace, the version
resolves on a registry.

**Packaging.** Both crates carry the licence and a README written for somebody
arriving from crates.io rather than from this repository, plus keywords and
categories. `cargo package --list` is clean for both.

**`cargo publish --dry-run` passes for `hlin-manifest` and cannot pass for
`hlin-view`.** Not a defect and not fixable here: the dry run resolves
dependencies against the registry, and `hlin-manifest` is not on it. The
acceptance criterion asked for something the ordering of crates.io does not
allow. What this establishes is the release order, which is a real finding:
`hlin-manifest` first, then `hlin-view`, and `hlin-view` cannot be verified end
to end until the first one exists.

### Two decisions for a person

**The names.** The convention Aurora follows is org-prefixing, and its own
reason is stated in its manifest: so it does not claim a generic name in a flat
namespace. `aurora` is generic. `hlin` is not, and `hlin-manifest`, `hlin-view`
and `hlin` are all free on crates.io right now. Following the spirit rather than
the letter argues for keeping the short names. Following the letter argues for
`colliery-io-hlin-view`. This is recorded rather than decided, and the crates
keep their current names until somebody says otherwise, because renaming costs
churn across every consumer and buys nothing before publication.

**`version = "0.0.1"`.** Publishing at `0.0.1` says nothing is promised, which
is currently true. The moment Aurora depends on it, the interface has consumers
and the version should say so. Worth deciding deliberately at release rather
than shipping the workspace's placeholder.

Nothing published. Checks clean, 269 Rust tests passing.
