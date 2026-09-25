---
id: with-no-public-url-use-the-origin
level: task
title: "With no public_url, use the origin a request arrived on"
short_code: "HLIN-T-0079"
created_at: 2026-09-25T02:21:48.753355+00:00
updated_at: 2026-09-25T02:21:48.753355+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/todo"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# With no public_url, use the origin a request arrived on

## Parent Initiative

[[HLIN-I-0011]]

## Objective

`Config::origin()` falls back to `http://localhost:{port}` when neither
`public_url` nor `oidc`'s is set. The module CSP and the request proxy's
`Origin` check both name that origin exactly, so a shell opened at
`127.0.0.1` (which is how the demo is opened) refused every module frame.
[[HLIN-T-0066]] worked around it by setting `public_url` in the demo configs;
anyone opening an unconfigured shell by another name still gets no modules,
and no reason.

With no `public_url` configured, the shell uses the origin the request
arrived on (its `Host`, with the scheme the shell is served over). A
configured `public_url` always wins. Agreed by the owner on 2026-09-24, to be
done after the wave running HLIN-T-0068, HLIN-T-0070 and HLIN-T-0077.

## Acceptance Criteria

- [ ] With no `public_url`, the module CSP, the page's `frame-src` and the
      request proxy's `Origin` check use the request's own origin
- [ ] With `public_url` set (or `oidc`'s), behaviour is unchanged
- [ ] The request proxy still requires `Sec-Fetch-Site: same-origin`, so a
      request cannot choose its own origin to pass the check
- [ ] Tests: `localhost` and `127.0.0.1` both work unconfigured; a configured
      `public_url` refuses a request whose `Origin` differs; a spoofed `Host`
      gains nothing a same-origin browser request would not already have
- [ ] The workaround `public_url` lines in `demo/*.toml` are removed or kept
      with a comment saying why
- [ ] `angreal check all`, `angreal test all`, both e2e flavours pass

## Implementation Notes

- `crates/hlin/src/config.rs` (`origin`), `modules/assets.rs` (`module_csp`,
  `page_csp`), `modules/requests.rs` (step 1), and the relay endpoint added in
  [[HLIN-T-0067]], which uses the same check.
- A `Host` header is caller-controlled, so reason it through: it only fills in
  when nothing is configured, and every check it feeds also needs a
  same-origin browser request.

## Status Updates

### 2026-09-24

Created. Queued after the current wave.
