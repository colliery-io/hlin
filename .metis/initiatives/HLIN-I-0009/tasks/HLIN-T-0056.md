---
id: the-chart-tells-a-default-install
level: task
title: "The chart tells a default install the truth about its database"
short_code: "HLIN-T-0056"
created_at: 2026-09-09T01:14:39.172101+00:00
updated_at: 2026-09-09T01:14:39.172101+00:00
parent: HLIN-I-0009
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"

exit_criteria_met: false
initiative_id: HLIN-I-0009
---

# The chart tells a default install the truth about its database

## What

With `postgres.enabled: true` — the default — NOTES.txt still prints:

> No database configured. The shell will run and warn: contract memory and the
> layouts people compose do not survive a restart...

Wrong twice. The chart provisioned a Postgres and wired `database_url_env`, and
a release build now refuses to start without one rather than warning.

The condition only tests `config.databaseUrl` and `config.databaseUrlSecret`,
so it fires for the bundled database it does not know about.

## Done when

- A default install's notes say where its database came from.
- An install with `postgres.enabled=false` and no URL is told it will not start
  — which is what the chart's own `hlin.validate` already refuses, so this
  branch should be unreachable; say so or delete it.
- Checked by rendering, not by reading.

## Status Updates

- 2026-09-09: Done. The notes now say where the database came from — the
  bundled Postgres and the Secret holding its generated password, or the Secret
  an operator named. The old branch fired on a default install and was wrong
  twice over.
- Also rewritten: the no-platforms note now shows the YAML rather than naming
  a key, and there is a new note pointing at `config.frontendImage` for
  somebody who wants their own design pack.
