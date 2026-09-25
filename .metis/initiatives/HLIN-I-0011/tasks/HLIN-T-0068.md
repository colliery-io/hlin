---
id: stream-a-response-to-a-module-as
level: task
title: "Stream a response to a module, as fast as it asks and no faster"
short_code: "HLIN-T-0068"
created_at: 2026-09-25T00:01:06.693811+00:00
updated_at: 2026-09-25T02:19:27.121815+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: false
initiative_id: HLIN-I-0011
---

# Stream a response to a module, as fast as it asks and no faster

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 8 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Streaming* and REQ-3.6.

## Acceptance Criteria

## Acceptance Criteria

- [ ] `fetch` with `stream: true` on a read yields `response`
      (`streaming: true`), then ordered `chunk`s, then `end`; on a write it is
      refused with `method`
- [ ] Credit: the page reads only as many bytes as the module has `pull`ed;
      a module that stops pulling stops the read, and backpressure reaches the
      platform (a test proves nothing buffers without bound)
- [ ] `cancel` aborts the request and the shell drops the upstream connection
- [ ] `end` errors: `idle` (`stream_idle_seconds`), `rate`
      (`stream_bytes_per_second`), `unreachable`, `cancelled`, `unmounted`
- [ ] Caps: `streams` per frame; page-wide 4 under HTTP/1.1 and 32 under
      HTTP/2+, read from the navigation's `nextHopProtocol`; past either,
      `too_many`
- [ ] At `/p/`: streamed responses pass through as they arrive; the upstream
      timeout applies until headers only; `response_bytes` does not apply
- [ ] The operator guide says to serve the shell over HTTP/2 for streaming
      modules
- [ ] Tests, including a browser test with a server-sent event source on the
      sample platform
- [ ] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0065]] and [[HLIN-T-0066]]. The SDK side is in
  [[HLIN-T-0069]].

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.
