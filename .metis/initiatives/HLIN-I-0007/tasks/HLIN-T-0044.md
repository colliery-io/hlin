---
id: subscribe-to-a-platform-s-event
level: task
title: "Subscribe to a platform's event stream, and notice when it goes quiet"
short_code: "HLIN-T-0044"
created_at: 2026-09-08T11:03:19.047724+00:00
updated_at: 2026-09-08T11:46:17.727185+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# Subscribe to a platform's event stream, and notice when it goes quiet

## Parent Initiative

[[HLIN-I-0007]]

## Objective

The shell subscribes to a platform's event stream, parses it, reconnects with
backoff when it drops, and — the part with no signal of its own — treats silence
past two heartbeat intervals as a drop.

One connection per platform, held only while a live surface holds a panel that
platform offers (REQ-2.1).

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] A subscriber that connects, parses `changed` events, and hands them to a
      caller-supplied sink
- [ ] Silence longer than twice the heartbeat interval closes and reconnects,
      and a test proves it against a server that goes quiet without closing
- [ ] Reconnection backs off on the schedule the registry already uses, rather
      than a second one invented here
- [ ] The subscribe request carries the platform's credential, so `hlin-token`
      mints into it exactly as it does for a data fetch
- [ ] Malformed events, unknown event types and unparseable JSON are dropped
      without closing the stream
- [ ] The stream is read incrementally with a per-event bound — never
      `read_bounded`, which is for bodies that end

## Implementation Notes

### Technical Approach

A new module in `crates/hlin/src/stream/`. `reqwest`'s `bytes_stream()` gives an
incremental body; SSE framing is small enough to parse directly and the shell
already emits the format, so the two halves can share a definition of it.

The bound deserves care: [[HLIN-T-0029]] capped upstream bodies before allocating
them, and an event stream is a body that never ends. The cap belongs on a single
event and on the unterminated-line buffer, not on the response.

### Dependencies

[[HLIN-T-0042]]. Easier to verify with [[HLIN-T-0043]] present.

### Risk Considerations

A silently wedged connection is the failure this exists to catch, so the test
for it has to hold a real socket open and say nothing. A mocked one would pass
against code that does not work.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

A subscriber in `crates/hlin/src/stream/events.rs`: connect, parse, and give up
in every way a platform can make it necessary. Eight socket-level tests and six
parser tests, and the whole file runs in 0.31 seconds.

**Two design choices worth recording.**

*Silence is a parameter, not a constant.* The failure this exists to catch takes
forty seconds to notice in production, and a test that takes forty seconds is one
somebody eventually deletes. Passed in, for the reason the aggregator takes a
clock rather than calling `Utc::now`.

*The subscription ends when nobody holds the receiver.* That is the whole
lifecycle in one line — the connection lives exactly as long as somebody is
listening, so a shell watching nothing holds nothing open (REQ-2.1) — and it
needs no separate bookkeeping to get right.

**Who the shell subscribes as.** An event stream is per platform, not per viewer:
it carries no data, so nothing in it could be one person's and not another's. The
shell subscribes as itself. The consequence is worth stating rather than
discovering later: a platform using `forward-session` cannot be subscribed to at
all, because the shell has no session of its own and must not borrow a viewer's —
that would make one person's cookie the credential for a connection serving
everybody. Such a platform is polled, which is what it did before this existed.

**A bug the parser had, and how it survived a passing test run.** A block ends
with a blank line, which has no colon, and `split_once(':')?` treated that as a
malformed event — so *every* well-formed event was discarded. The unit test that
catches it was already written and already correct; I had run
`cargo test --test events` and not `--lib`, so it never ran. Fixed, and lines
without a colon are now skipped rather than fatal, which is also what the format
says they mean.

**The bound.** A held-open response is a body that never ends, so the shell's
usual rule from [[HLIN-T-0029]] cannot apply to the response. It applies to one
event and to the buffer holding a partial one, with a test that opens a stream
and sends a single endless line.
