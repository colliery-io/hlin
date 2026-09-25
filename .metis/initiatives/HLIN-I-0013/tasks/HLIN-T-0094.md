---
id: twenty-widget-containers-a-shell
level: task
title: "Twenty widget containers, a shell and Dex in one compose file"
short_code: "HLIN-T-0094"
created_at: 2026-09-25T23:54:59.675828+00:00
updated_at: 2026-09-25T23:54:59.675828+00:00
parent: HLIN-I-0013
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0013
---

# Twenty widget containers, a shell and Dex in one compose file

## Parent Initiative

[[HLIN-I-0013]]

## Objective

Bring up the intended deployment on one machine: twenty widget containers
each on its own `*.comp.test` name, the shell as a release build on `oidc`
against Dex, and Postgres, all on one compose network with only the shell's
port published.

## Acceptance Criteria

- [ ] A Dockerfile whose builder stage compiles the shell, its frontend, every
      widget binary, every module and the core UI once (release, `wasm-opt`),
      and whose final stages give one small image per widget and one for the
      shell; BuildKit caching so an unchanged rebuild is quick
- [ ] A compose file (its own, or a profile in `docker-compose.yml`) with
      Postgres, Dex, `hlin.comp.test` and twenty widgets as
      `<name>.comp.test`, each serving its core UI at `/` and Hlin under
      `/hlin`, healthchecks, and only the shell published
- [ ] The shell's config lists the twenty with
      `base_url = "http://<name>.comp.test:8080/hlin"` on `hlin-token`, and
      `public_url` for the published address; Dex's static client matches
- [ ] An angreal way to bring it up and down (e.g. `demo up --with
      twenty-compose`), which publishes "Twenty" by signing in through Dex as
      the collab flavour does, and prints how to open it
- [ ] A widget can be stopped and started (`docker compose stop clock`) as a
      platform going down and coming back
- [ ] Build time cold and warm recorded; image sizes recorded
- [ ] `angreal check all`, `angreal test all` pass

## Implementation Notes

- Depends on [[HLIN-T-0093]]. The repository's existing `Dockerfile` builds
  the shell image with a frontend; build on it rather than beside it.
- Dex and its config already exist for the collab flavour.

## Status Updates

### 2026-09-25

Created. Not started.
