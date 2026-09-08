---
id: hlin-manifest-manifest-types
level: task
title: "hlin-manifest: manifest types, validation, canonicalization, contract hash and diff"
short_code: "HLIN-T-0002"
created_at: 2026-09-07T12:13:53.322650+00:00
updated_at: 2026-09-07T12:53:19.248967+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# hlin-manifest: manifest types, validation, canonicalization, contract hash and diff

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Encode the manifest schema ([[HLIN-S-0001]]) as the executable form of the spec in `hlin-manifest`: the document types, validation with per-panel isolation, path resolution, RFC 8785 canonicalization, the SHA-256 contract hash, and diff classification between two manifests. Every rule in the spec's requirement tables becomes a test.

## Acceptance Criteria

## Acceptance Criteria

- [x] Types for `Manifest`, `Platform`, `NavigationEntry`, `Panel`, `ParamDecl` (string shorthand or object form), `Lifecycle`; unknown fields preserved on round-trip at every level (NFR-1.3)
- [x] The spec's example manifest deserializes, re-serializes, and re-deserializes to an equal value
- [x] `validate(manifest, expected_platform_id)` returns document-level outcome (valid / malformed with reason) plus a per-panel outcome list; a defective panel rejects only itself (REQ-3.2); duplicate keys reject every panel sharing the key
- [x] Path resolution strips a leading `/`, rejects absolute URLs, scheme-relative URLs, and `..` segments (REQ-1.2), with tests for each
- [x] `platform.id` mismatch with the expected id is document-level malformed (REQ-1.5)
- [x] `contract_hash(manifest)` is stable under key reordering and whitespace, excludes `kind`, `title`, `description`, `navigation`, `health`, and `contract_version`, normalizes params to object form and sorts them, and materializes defaults
- [x] `classify_diff(previous, next)` yields Additive / NonContract / Breaking per the spec's table, with one test per row, and reports `Violation` (breaking without major bump) and `Rollback` (version decreased) as specified in [[HLIN-A-0002]]
- [x] Debounce is explicitly *not* implemented here (registry concern) and documented as such, in the `diff` module docs and in a test that shows flapping classifying independently each time

## Implementation Notes

### Technical Approach
`serde` with `#[serde(flatten)] extra: BTreeMap<String, Value>` for unknown-field preservation; `semver` for `contract_version`; a JCS implementation (`serde_json_canonicalizer` or in-crate) feeding `sha2`. Validation returns structured reasons that map directly onto the spec's degradation table so the registry can classify without re-parsing messages.

### Dependencies
[[HLIN-T-0001]].

### Risk Considerations
JCS number formatting must match RFC 8785 exactly or hashes differ across implementations (NFR-1.2); add a test vector from the RFC.

## Status Updates

**2026-09-07 — complete.** `hlin-manifest` now carries the manifest half of the contract, in seven modules: `manifest` (types), `path` (path resolution), `params` (the parameter vocabulary), `validate` (two-tier validation), `canonical` (contract content, canonicalisation, hash), `diff` (change classification and verdict), and `errors` (parsing). 47 tests plus a doctest; `angreal check all` clean with clippy at `-D warnings`.

Scope decisions taken during the work:

- **The parameter vocabulary landed here, not in `hlin-view`.** Validating a panel means answering "is this a real parameter, and is it configured correctly", which needs the vocabulary. Parameters are contract rather than rendering: what a control is called, what configuration it takes, and how its value is encoded onto a data request are things a platform must agree with the shell about, whereas how a time picker *looks* is not. `hlin-view` keeps the view kinds and the acceptance matrix, per [[HLIN-A-0005]]. `params.rs` implements `time_range` and `select`, reserves `from`/`to`/`step`, and owns the narrowing judgement the diff asks for.
- **Validation is total, and returns outcomes rather than errors.** `ParseError` covers only documents that are not readable JSON. Everything a document says that is merely wrong is a classified `DocumentDefect` or `PanelDefect`, structured so the registry can act without parsing a message.
- **Parameters are matched by identity across revisions**, not by position, so reordering a panel's controls is not a change. A `select` is identified by the query key it claims, which makes two selects on one panel distinguishable.
- **`ParamDecl` serialises back to shorthand when it has no configuration.** Both wire forms parse to one type and canonicalisation always uses the object form, so the two forms fingerprint identically.
- **Validation rejects a few things the specification did not spell out**: an empty title, kind or envelope; a sunset on a panel that is not deprecated; a successor that names a panel not in the manifest; a panel naming itself as successor; the same parameter declared twice. Each rejects one panel only. If any of these should be tolerated instead, they are one-line changes in `validate::panel_defect`.

On the risk this task recorded, JCS number formatting: `serde_jcs` was verified against RFC 8785 by hand and matches on all three axes that implementations usually differ. Numbers use ECMAScript's shortest round-tripping form (`-0` → `0`, `1e30` → `1e+30`, `1e20` → full digits, `333333333.33333329` → `333333333.3333333`); object keys sort by UTF-16 code unit, so a supplementary-plane character sorts before U+FF3A where its UTF-8 encoding would sort after; strings use the two-character escapes where they exist. Those vectors are pinned in `canonicalisation_follows_rfc_8785` so a dependency regression is caught. This matters despite contract content being mostly strings, because unknown parameter configuration fields are preserved and hashed, so a platform can put a number into the fingerprint.

Suggested follow-up, not blocking: `Validation` reports the newer-schema-version case as an informational field. The registry will need to decide what an operator sees for that, alongside the violation signal.
