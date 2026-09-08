---
id: 001-manifest-versioning-is-content
level: adr
title: "Manifest versioning is content hash plus semver, enforced at runtime by the aggregator"
number: 1
short_code: "HLIN-A-0002"
created_at: 2026-08-04T14:24:43.427755+00:00
updated_at: 2026-08-04T14:26:38.340697+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-2: Manifest versioning is content hash plus semver, enforced at runtime by the aggregator

## Context

The vision ([[HLIN-V-0001]]) says panel contracts are "versioned, diffed in CI, and deprecated with a window and a named successor," and that removing a panel key without a major version is a breaking change that fails the build attempting it. Two questions fall out: what the version actually is, and who enforces it.

A single version number conflates two different facts: *did the contract change at all* (relevant for caching, re-validation, change detection) and *did it change in a way consumers must care about* (relevant for breakage). And requiring an external CI service to enforce the contract would add a coordination point the architecture is designed to avoid.

There is a separate, orthogonal axis: the manifest *document format* itself carries a monotonic, additive `schema_version` with unknown fields ignored (per the vision). That axis is shell-owned and unaffected by this decision, which concerns each platform's own panel contract.

## Decision

Each platform's panel contract is versioned on two mechanisms:

- **A content hash**, computed over the canonicalized contract content of the manifest (panel keys, kinds, envelopes, params, endpoints, lifecycle entries). The hash answers "did anything change." The aggregator computes it itself from the fetched manifest, so it cannot be forgotten or faked by a platform.
- **A semver `contract_version`** declared in the manifest. Semver expresses intent: a major bump declares a breaking change (panel key removal, parameter or envelope narrowing); minor/patch declare additive or incidental change.

**Enforcement is runtime, in the shell: the shell is effectively CI, and the renderer is CD.** On each poll the shell compares the manifest against the last-seen one: hash unchanged means nothing happened; hash changed with an additive diff is accepted silently; a breaking diff without a major bump is a contract violation, raised as an operator signal — never a shell error. Panels that pass flow to users immediately through the renderer. No additional service exists to "compile" or gate the composed frontend.

Platforms remain free to diff their own manifests in their own CI (and should), but nothing in Hlin depends on their having done so.

### Refinements (2026-09-07, after design review)

- **Semver is descriptive, not enforced beyond the major.** The hash states what *is* true; `contract_version` states what the platform *claims* is true. The only mechanically checked claim is "no breaking change without a major bump." Minor and patch components are informational; an additive change with no version bump at all is not a violation.
- **Violations never degrade panels.** A breaking change without a major bump is the platform's versioning sin, and the person looking at the dashboard should not pay for it. The shell renders the new declaration as delivered (removed panels are simply gone and show as unavailable/unknown in layouts that referenced them) and raises an operator signal naming the platform, the diff, and the expected bump.
- **Version regression is a rollback, not a violation.** A `contract_version` lower than the last seen is treated as a rollback: the diff is applied and logged at informational level, never flagged. Rollbacks happen during incidents, exactly when the shell matters most.
- **Diffs are debounced.** A new contract hash is classified only after it has been observed on a shell-configured number of consecutive polls (default two), so blue/green and canary rollouts that make the manifest flip-flop do not generate a violation per flip.
- **The registry does the diffing.** Mechanically, the component holding the sequence of manifests for a platform is the registry, so contract identity, diffing, and violation detection live there; the aggregator consumes validated contracts. "The shell is CI" is the architectural statement; the registry is where it is implemented.
- **Supersedes** the vision sentence "fails the build that attempts it" ([[HLIN-V-0001]], Principles): there is no build to fail. The vision should be amended to "is detected and flagged by the shell at runtime."

## Rationale

The hash and the semver answer different questions, and each is wrong for the other's job: a semver alone cannot detect an undeclared change (the exact failure mode we care about), and a hash alone cannot express intent. Together they make the interesting case mechanically detectable: *contract changed, breakage not declared*.

Runtime enforcement is the only placement consistent with platform autonomy. A central CI gate over a dozen independently released platforms is exactly the coordination bottleneck the vision rejects; the aggregator already fetches every manifest and already owns the panel state machine ([[HLIN-A-0001]]), so it is the natural — and only — component positioned to validate every contract on every change with no new infrastructure.

## Consequences

### Positive
- Undeclared breaking changes are detected mechanically, on the running system, within one poll interval — not left to the discipline of twelve teams' CI configs.
- No shared build or gate service; a platform deploy is the entire release process, as the vision requires.
- The hash gives the aggregator a free change-detection primitive for caching and re-validation.

### Negative
- Violations are detected *after* deploy, not before. The feedback loop for a platform that ships a breaking change without a major bump is an unavailable panel and an operator signal, not a red build. Platforms wanting pre-deploy safety must run their own manifest diff in their own CI.
- Canonicalization must be specified precisely (field ordering, defaults, what is and is not contract content) in the manifest specification, or hashes will disagree across implementations.

### Neutral
- The last-seen manifest becomes aggregator state that must survive restarts, or violations occurring across a restart window go undetected.
