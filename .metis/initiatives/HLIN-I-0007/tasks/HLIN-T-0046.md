---
id: a-pushed-panel-polls-at-the
level: task
title: "A pushed panel polls at the relaxed interval, and falls back when the stream does"
short_code: "HLIN-T-0046"
created_at: 2026-09-08T11:03:19.088447+00:00
updated_at: 2026-09-08T12:04:02.935509+00:00
parent: platforms-tell-the-shell-when-data
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0007
---

# A pushed panel polls at the relaxed interval, and falls back when the stream does

## Parent Initiative

[[HLIN-I-0007]]

## Objective

A pushed panel on a connected stream polls at `max(panel refresh_ms, shell
refresh_ms)` instead of its own cadence — and goes straight back to its own
cadence the moment the stream is not connected.

This is where the requests are actually saved, and where the feature could do
harm if it is wrong.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [ ] The interval table in HLIN-S-0006 holds: relaxed only when the stream is
      connected *and* the panel is `pushed`; `refresh_ms` in every other case
- [ ] Losing the stream returns every panel of that platform to `refresh_ms`
      with no recovery step, because the poll was never turned off (REQ-1.4)
- [ ] A panel that is not `pushed` on a platform that has a stream is never
      relaxed
- [ ] A shell whose platforms declare no streams makes exactly the same requests
      it made before this initiative — asserted by counting them (REQ-1.1)
- [ ] The demo's upstream request rate is measured before and after, and the
      number goes in the repository rather than in a commit message

## Implementation Notes

### Technical Approach

The cadence decision already lives in one place from [[HLIN-T-0036]]: the clamp
in `aggregator`. This adds one input to it — whether this panel is currently
being reported on — rather than a second decision somewhere else.

### Dependencies

[[HLIN-T-0044]] and [[HLIN-T-0045]].

### Risk Considerations

The dangerous mistake is relaxing on "the platform has a stream" rather than
"the stream is connected right now", which would leave panels stale whenever a
platform's stream was down. The test for it is a platform that declares a stream
and refuses the connection.

## Status Updates

*To be added during implementation*
### 2026-09-08 — Done

The interval table from HLIN-S-0006, and the measurement that says it was worth
building.

**Measured on the demo, one surface, one minute, same panel both times:**

| | upstream requests per minute |
|---|---|
| polled at its declared cadence | 156 |
| with its platform's stream connected | 14 |

Both measured rather than derived, and recorded in [[HLIN-S-0006]] rather than
in a commit message. The panel is also *more* current under push, not less: a
change is fetched when it happens rather than up to 250ms later.

**Relaxing follows the socket, not the manifest.** `Surface::streaming_from`
takes a fact the driver reports, and a stream going down puts every panel of
that platform straight back on its declared cadence with no recovery step —
because the poll was never turned off. Five aggregator tests cover the table,
including the one that matters most: a shell whose platforms offer nothing makes
*exactly* the requests it used to, asserted by counting.

**Two defects found by measuring rather than by testing.**

*The shared client killed every subscription.* The shell builds one
`reqwest::Client` with the upstream timeout, which is right for a data fetch and
fatal for a held-open response: a request timeout covers the whole exchange
including the body, so every subscription died after ten seconds and reconnected
forever. The first measurement read 78 requests in 30 seconds — a number that
looked like a modest saving and was actually the feature not working at all. A
second client, with a connect timeout and no request timeout, is on `AppState`.

*A connected stream has to announce itself.* Marking a platform as streaming
when its first event arrives would never relax a panel on a platform that is
reporting perfectly and simply has nothing to say — which is the ordinary case.
`follow` now signals as soon as the response head is accepted, before anything
has arrived.

**The subscriptions are a drop guard.** `Following` aborts them when the driver
returns. The driver has several exits, one of them inside a lock, and the
version that tidied up by hand would have leaked on whichever path somebody
forgot — the same leak [[HLIN-T-0027]] fixed for surfaces, in a new place.

359 Rust tests, 24 browser tests, walkthrough holds.
