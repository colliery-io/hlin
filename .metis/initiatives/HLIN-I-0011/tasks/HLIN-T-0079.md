---
id: with-no-public-url-use-the-origin
level: task
title: "With no public_url, use the origin a request arrived on"
short_code: "HLIN-T-0079"
created_at: 2026-09-25T02:21:48.753355+00:00
updated_at: 2026-09-25T11:20:42.704887+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: true
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

## Acceptance Criteria

- [x] With no `public_url`, the module CSP, the page's `frame-src` and the
      request proxy's `Origin` check use the request's own origin
- [x] With `public_url` set (or `oidc`'s), behaviour is unchanged
- [x] The request proxy still requires `Sec-Fetch-Site: same-origin`, so a
      request cannot choose its own origin to pass the check
- [x] Tests: `localhost` and `127.0.0.1` both work unconfigured; a configured
      `public_url` refuses a request whose `Origin` differs; a spoofed `Host`
      gains nothing a same-origin browser request would not already have
- [x] The workaround `public_url` lines in `demo/*.toml` are removed or kept
      with a comment saying why
- [x] `angreal check all`, `angreal test all`, both e2e flavours pass

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

### 2026-09-25

Done. `Config::origin()` is now only the configured origin (`public_url`, else
`oidc`'s), as an `Option`; `Config::origin_for(headers)` resolves one
request's, and the module CSP, the page's `frame-src` (now built per request in
`with_frontend`), the request proxy's step 1 and the `changed` relay all use
it. Spec HLIN-S-0007 has a new *The shell's origin* section with the reasoning.

Decisions:

- **Scheme** is always `http` when unconfigured: the shell serves plain http
  itself. `X-Forwarded-Proto` and relatives are not read, because nothing
  unconfigured says which proxy may be believed; a shell behind TLS sets
  `public_url`.
- **Host** is used only when it is plainly a host and port (alphanumerics,
  `.-:[]`), since it goes into a CSP header; otherwise, or when absent,
  `http://localhost:{port}` as before.
- **Spoofed Host** gains nothing: read only when unconfigured; a policy built
  from it goes back only to whoever chose it; the proxy and relay still need
  `Sec-Fetch-Site: same-origin`. Noted: a host name somebody else points at the
  shell (DNS rebinding) is a page the browser treats as the shell's, which
  could already call its API; an exposed shell sets `public_url`.
- **Demo configs**: the HLIN-T-0066 workaround is removed from `hlin`,
  `aurora`, `gallery`, `live` and `twenty` (separate commit), with a comment
  saying why there is none. `collab` keeps its `public_url`: it is `oidc`'s,
  which the redirect URI needs.

Verified: `angreal check all`; `angreal test all` (819 passed, 3 ignored);
e2e standard against `--with aurora` at `http://127.0.0.1:8080` (53 passed,
20 skipped) and again with `HLIN_URL=http://localhost:8080` (53 passed, 20
skipped); `angreal e2e signin` against `--with collab` (9 passed).
