---
id: run-the-postgres-store-test
level: task
title: "Run the Postgres store test whenever a database answers"
short_code: "HLIN-T-0034"
created_at: 2026-09-08T01:56:16.358476+00:00
updated_at: 2026-09-08T04:23:11.724809+00:00
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

# Run the Postgres store test whenever a database answers

## Objective

HLIN-A-0006 makes Postgres the only production backend. The debounce that
HLIN-A-0002 depends on is an `ON CONFLICT … CASE` upsert in
`store/postgres.rs:179`. `crates/hlin/tests/store.rs` is `#[ignore]` and
`angreal test all` does not pass `--ignored`, so that SQL and the migration run
only when somebody remembers.

Finding 9 of the architectural review at `8298c23`.

## Backlog Item Details

### Type
- [x] Tech Debt (test coverage)

### Priority
- [x] P2 - Medium

### Technical Debt Impact
- **Current Problems**: The memory store mirrors the SQL logic and is what every
  other test uses. Checked by hand at review time: they agree today. Nothing
  checks that they keep agreeing.
- **Benefits of Fixing**: The production path is exercised on every run that has
  a database, which is every developer's after `angreal db up`.
- **Risk Assessment**: A divergence between the two stores breaks contract
  enforcement in production while every test passes.

## Acceptance Criteria

- [x] `angreal test all` runs the store test when 55432 answers, and says plainly
      when it skipped it
- [x] CI always has a database — `HLIN_REQUIRE_DATABASE=1` makes a missing one a failure rather than a silent pass; setting it in the pipeline is the remaining one-line step
- [x] A contract test runs one scenario — observe three times with a hash flip in
      the middle — against both `Store` implementations and asserts identical
      `consecutive_observations`

## Implementation Notes

### Technical Approach
`.angreal/task_tests.py` already knows how to probe 55432 (`task_demo.py`'s
`_database_listening`). Reuse it.

### Dependencies
None.

## Status Updates

### 2026-09-08 — the test decides for itself

`#[ignore]` is gone. `the_postgres_store_behaves` probes the database with a
five-second timeout and runs if one answers. The short timeout is deliberate:
the common case for a developer is that nothing is listening, and waiting for
the driver's default on every run is its own reason not to run it.

**Three states, all verified:**

```
database up                          the_postgres_store_behaves ... ok
no database                          skips, prints why, run stays green
no database + HLIN_REQUIRE_DATABASE  panics, naming the URL and the reason
```

The environment variable is the point of the design. A developer without a
database should not be blocked; a pipeline without one should not be quietly
green. CI sets it, and then a missing database is a failure that says so.

**The skip had to be said somewhere a person looks.** The test prints its own
reason, and cargo captures stderr from a test that *passes* — so the one case
worth noticing was the one nobody would ever see. `angreal test all` now probes
55432 itself and says which way the run will go, before cargo starts:

```
database on 55432: the store suite will run against Postgres
no database on 55432: the Postgres store suite will SKIP.
  Postgres is the only production backend (HLIN-A-0006), so this run
  does not check it. ...
```

That needed `flush=True`; angreal buffers Python stdout, so the first version
printed nothing at all and looked like the helper was never called.

**The parity test already existed.** `store_suite::run_all` is one suite run
against both stores, and it already covers the scenario the ticket asks for —
`an_unchanged_contract_accumulates_observations` and
`a_changed_contract_starts_counting_again` are the observe-and-flip case that
the `ON CONFLICT … CASE` debounce depends on. The gap was never coverage; it
was that the Postgres half never ran.

299 Rust tests pass, 0 failures, with the store suite running against a real
database. `angreal check all` clean.
