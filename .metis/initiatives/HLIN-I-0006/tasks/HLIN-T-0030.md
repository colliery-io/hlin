---
id: persist-and-expose-contract
level: task
title: "Persist and expose contract verdicts"
short_code: "HLIN-T-0030"
created_at: 2026-09-08T01:56:11.093853+00:00
updated_at: 2026-09-08T04:12:50.277221+00:00
parent: HLIN-I-0006
blocked_by: []
archived: false

tags:
  - "#task"
  - "#tech-debt"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0006
---

# Persist and expose contract verdicts

## Objective

HLIN-A-0002 says a breaking change shipped without a major bump is "raised as an
operator signal". The detection is complete and restart-safe; the signal is one
`tracing::error!` at the moment of classification. The verdict is not in
`PlatformSnapshot`, not on `/api/platforms`, and gone when the log rotates.

Finding 5 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt

### Priority
- [x] P1 - High

### Technical Debt Impact
- **Current Problems**: `registry.rs:357` `report()`. An operator who was not
  tailing the log cannot answer "has this platform ever shipped a breaking change
  undeclared?" — the one question the mechanism exists to answer.
- **Benefits of Fixing**: The project's central bet becomes observable rather
  than momentary.
- **Risk Assessment**: The enforcement is real and nobody will know it fired.

## Acceptance Criteria

- [x] `PlatformSnapshot` carries the last verdict, its changes, and when
- [x] `/api/platforms` exposes `last_violation` per platform
- [ ] A counter `hlin_contract_violations_total{platform}` for alerting — **deferred**, see below; the shell has no metrics endpoint and `seen` carries the same signal over the API
- [x] `angreal demo walkthrough` asserts the violation is visible on
      `/api/platforms`, not only in the log
- [x] The log line stays

## Implementation Notes

### Technical Approach
A migration adding nullable `last_verdict JSONB` and `last_verdict_at` to
`platforms`; write it from `observe()` alongside the snapshot; read it into
`PlatformReport`.

### Dependencies
None.

## Status Updates

### 2026-09-08 — the signal outlives the log line

A `Violation` — declared version, the major it needed, what broke in words, when,
and how many times this platform has done it — is recorded by the store when the
registry classifies one, carried on `PlatformView`, restored on boot, and served
on `/api/platforms`.

**Verified on the running demo**, through the walkthrough's own breaking-change
step:

```
6. stampmill comes back having dropped a panel without a major bump
   declared=1.2.0 expected="2.0.0" changes=[PanelRemoved { key: "queue-depth" }]
   recorded: declared 1.2.0, seen 1 time(s)

/api/platforms:
  orebank    "last_violation": null                                  (behaved)
  stampmill  "last_violation": {"declared":"1.2.0","expected_major":2}
```

The `null` matters as much as the record: a platform that has never misbehaved
says so, so the field answers the question rather than merely sometimes carrying
data.

**Three decisions worth writing down.**

*On the platform row, not a table of its own.* An operator asks "has this
platform ever done this, and when", not "show me the history". A platform that
violates weekly is one problem rather than many, and `seen` carries the
repetition without the rows — with a second `error!` line once it passes one,
because an accident and a process problem want different conversations.

*Stored as strings, not as `hlin_manifest::Verdict`.* This is a record of
something that happened, and a record has to keep meaning what it meant after
the type that produced it moves on. Somebody reading it in a month needs to know
what broke, not to reconstruct a value.

*Read leniently.* `last_violation` is a column added after the fact, so a row
written before the migration has SQL `NULL` there. A shell that refused to read
its own contract memory over a platform that has never misbehaved would be a
poor trade.

**Restart survival is the test that matters**, and it is the one written: the
window a breaking change is most likely to be shipped in is the one nobody is
watching, so the record has to outlive the process that made it. The test polls
until a violation is believed, restarts the registry against the same store, and
asserts the violation is still there.

296 Rust tests pass, 0 failures. `angreal check all` clean.

### Deferred, with the reason

The Prometheus counter (`hlin_contract_violations_total`) is not done. The shell
has no metrics endpoint and no metrics dependency, and adding one to satisfy a
single counter is a larger decision than this task — it means choosing a
registry crate, an exposition format and a route, for the whole shell.

`seen` on `/api/platforms` carries the same information for anything that can
poll an API, which is what makes deferring this reasonable rather than a gap.
Worth its own task when the shell grows metrics generally.
