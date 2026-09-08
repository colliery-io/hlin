---
id: platform-event-streams
level: specification
title: "Platform Event Streams"
short_code: "HLIN-S-0006"
created_at: 2026-09-08T10:51:52.581317+00:00
updated_at: 2026-09-08T10:51:52.581317+00:00
parent: HLIN-I-0007
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"

exit_criteria_met: false
initiative_id: NULL
---

# Platform Event Streams

## Overview

Hlin's third contract, and the first one a platform may decline entirely.

[[HLIN-A-0010]] records that the shell polls, and what that costs: 20 upstream
requests a second for one surface of four panels at `refresh_ms = 125`, paid
continuously whether or not anything changed. [[HLIN-A-0011]] decides how to
remove the waste without giving up what polling buys — the shell subscribes to a
stream the platform declares, the stream carries *news rather than data*, and
polling continues underneath at a relaxed interval.

This specification says what that stream looks like, what the shell does with
it, and — most of the text below — how it behaves when it is absent, wrong, or
lying, because that is the part that decides whether the feature can be adopted
by a platform whose author the shell's operator has never met.

Scope is the **platform→shell notification path** only. The shell→browser stream
is [[HLIN-S-0003]] and is untouched by this: every frame a browser receives is
still produced by a fetch the shell issued, and every row of that document's
outcome table still describes it.

## System Context

### Actors

- **Platform**: optionally serves an event stream at a path it declares, and
  names which of its panels it reports on. Still knows nothing about the shell.
- **Shell**: subscribes to that stream while it has a reason to, translates
  events into "fetch this sooner than you were going to", and keeps polling.
- **Browser**: unaffected. It cannot tell whether a frame it received was
  prompted by a timer or by an event, and there is nothing it could usefully do
  differently if it could.

### Boundaries

Inside: the manifest fields, the event stream's shape and semantics, the
subscription lifecycle, cadence relaxation, coalescing, and every degradation
path. Outside: the envelope shapes ([[HLIN-S-0002]]), the identity a fetch
carries ([[HLIN-S-0005]]), and the browser protocol ([[HLIN-S-0003]]).

## Requirements

| ID | Requirement | Why |
|----|-------------|-----|
| REQ-1.1 | A platform declaring no event stream behaves exactly as it does today | Adoption must be opt-in per platform, or the feature is a breaking change to a contract twelve teams already implement |
| REQ-1.2 | The shell is correct when no event ever arrives | This is what makes delivery best-effort, and what removes every mechanism guaranteed delivery would need |
| REQ-1.3 | An event never carries panel data | A frame is a property of a panel *and* its viewer and their selections; a platform cannot produce one correctly (HLIN-A-0011) |
| REQ-1.4 | Losing the stream degrades to polling at the declared cadence, not to staleness | The poll was never turned off, so there is nothing to recover |
| REQ-1.5 | The event stream is excluded from the contract hash | The shell is correct without it by construction, so adding or withdrawing it is not a breaking change (HLIN-A-0002) |
| REQ-2.1 | One connection per platform, held only while some surface watches a panel it offers | A shell watching nothing holds nothing open |
| REQ-2.2 | An event for a panel nobody is watching is discarded without a fetch | Otherwise a platform's event rate, not the shell's viewers, sets the shell's load |
| REQ-2.3 | Fetches prompted by one event are coalesced and jittered | One event for a widely watched panel is many distinct fetches that the tick currently spreads out (HLIN-A-0011) |
| REQ-2.4 | The shell's refresh floor applies to event-prompted fetches | The shell pays for the load and keeps the last word, exactly as with `refresh_ms` (HLIN-A-0009) |

## Design

### What a platform declares

Two additions to the manifest ([[HLIN-S-0001]]), neither of them contract:

```json
{
  "platform": { "id": "orebank", "name": "Orebank" },
  "events": "api/events",
  "panels": [
    { "key": "queue-depth", "title": "Queue depth", "kind": "stat",
      "envelope": "scalar.v1", "data": "api/queue-depth",
      "refresh_ms": 5000, "pushed": true }
  ],
  "health": "api/health"
}
```

`events` is a path relative to the platform base, like `data` and `health`.
Absent means this platform is polled and nothing changes for it.

`pushed` on a panel means "I will tell you when this one changes". It is
required, and it is not redundant with `events`: without it the shell cannot
distinguish *no event yet because nothing changed* from *this panel is never
reported on*, and would relax the polling of a panel it will never hear about —
trading redundant requests for silent staleness, which is a worse deal than the
one it started with.

A platform declaring `events` but marking no panel `pushed` is well-formed and
means nothing; the shell does not connect.

### The stream

Server-sent events, for the reasons [[HLIN-S-0003]] gives for the browser leg
and which hold again here: traffic is one-directional, reconnection and event
ids come free, and it is ordinary HTTP that any proxy, gateway and platform
framework already handles.

```
event: changed
data: {"panel": "queue-depth"}

event: changed
data: {"panel": "throughput", "selections": {"cluster": ["west"]}}
```

One field is required: `panel`, a key from this manifest. A key the shell does
not recognise is ignored — a platform mid-deploy may serve a stream from a newer
revision than the manifest the shell last read, and dropping the event is
correct and silent.

`selections` is optional and narrows the claim: only instances whose selections
match are refetched. A platform that finds this hard should omit it —
over-reporting costs the shell requests it would mostly have made anyway, and
under-reporting is the failure that matters.

**Matching is by subset.** An event matches an instance when every key the event
names has a matching value on that instance; keys the event does not mention are
not consulted. So an event naming `cluster: ["west"]` reaches every instance
watching west whatever else they have selected, and an event naming no
selections at all reaches every instance of the panel — the empty case falls out
of the rule rather than needing one of its own.

Exact matching was the alternative and is wrong for any panel with more than one
control: a platform that knows the west cluster changed would have to enumerate
every combination of every other control a viewer might have set, and would
silently miss the ones it did not think of.

**A platform must emit a heartbeat.** An SSE comment line — `:` followed by a
blank line — at least every 20 seconds, whether or not anything changed.

This is the one thing WebSocket would have given for free, and the reason it is
required rather than encouraged. A connection can stop delivering without
closing: a NAT table forgets it, a proxy drops what looks idle, a platform
process wedges. Nothing announces any of those. Without a heartbeat the shell
sits holding a socket it believes is live, on the relaxed interval, indefinitely
— which is the one way this feature could leave a viewer worse off than plain
polling, and it would do it silently.

The shell treats silence longer than twice the heartbeat interval as a drop:
every panel of that platform returns to `refresh_ms` and the subscription
reconnects with backoff. A platform that cannot emit heartbeats should not
declare `events`; it would be asking the shell to poll less on a promise it
cannot be held to.

The shell sends `Last-Event-ID` on reconnect where the platform supplied ids,
and a platform is free to ignore it: a gap in events is exactly as harmful as no
events at all, which is to say one relaxed interval of staleness.

**Not in scope, deliberately.** Events do not carry data (REQ-1.3), do not
describe *how* something changed, and are not ordered relative to one another or
to fetches. The shell's only response to any event is to move a fetch earlier,
and a fetch is what it has always been.

### What the shell does

Per platform, while at least one live surface holds a panel that platform offers
**and** any of its panels are `pushed`:

1. Open the stream. Reconnect with backoff on loss, on the same schedule the
   registry already uses for a platform that will not answer.
2. On `changed`, find the panel instances on live surfaces that match, and mark
   them due — subject to the coalescing window below.
3. Close the stream when the last such surface stops. The surface lifecycle
   already exists ([[HLIN-T-0027]]); this hangs off it rather than inventing a
   second notion of "in use".

Per panel instance, the interval it is polled at:

| stream connected | panel `pushed` | interval |
|---|---|---|
| yes | yes | the relaxed interval |
| yes | no | `refresh_ms` |
| no | either | `refresh_ms` |

The relaxed interval is `max(panel refresh_ms, shell refresh_ms)`.

A rule rather than a new setting, and the two halves are both load-bearing.
Never faster than the panel asked for, because a panel declaring a slow cadence
meant it and push is not a reason to poll it more. Never slower than the shell's
own default interval, because that number is already the operator's answer to
"how stale may a panel get when nothing is telling us otherwise" — which is
exactly the question here.

On the demo that is 30 seconds for a panel declaring 125ms: two hundred and
forty polls a minute become two, and the shell's staleness threshold of 90
seconds still gives three chances to notice before anything on screen says so.

It is a safety net rather than a cadence. It exists so that a platform that
stops reporting — a bug, a partition, a queue it silently dropped — leaves
panels stale by a bounded amount rather than forever.

### Coalescing

An event for a panel twenty principals are watching with different selections is
twenty fetches the tick would have spread across an interval and an event would
fire together. The shell therefore holds event-prompted fetches for a short
window and issues them jittered across it, and collapses repeated events for the
same instance within that window into one fetch. The floor from HLIN-A-0009
applies unchanged: a platform emitting a thousand events a second gets the same
treatment as one asking for `refresh_ms = 1`.

## What it saves, measured

On the demo, one surface holding `orebank/batches` — a count that changes
irregularly about every four seconds, declaring `refresh_ms = 250` so that it
feels immediate under polling — watched for one minute:

| | upstream requests per minute |
|---|---|
| polled at its declared cadence | 156 |
| with its platform's event stream connected | 14 |

Both measured, not derived. The polled figure is lower than the 240 the cadence
implies because the driver's tick bounds how finely it can notice a refresh is
due; a panel at 125ms measures 312 rather than 480 for the same reason.

The 14 is two relaxed polls plus one fetch per change that actually happened.
And the panel is *more* current than the polled one, not less: a change is
fetched when it occurs rather than up to 250ms later.

That is the shape of the whole trade. Push is worth having exactly where data
must be seen promptly and changes seldom — where polling makes you choose
between being late and being wasteful. A panel whose data genuinely moves eight
times a second gains nothing from it, which is why the demo's `live-*` panels
declare a fast cadence and do not claim to be pushed.

## What an operator can see

`/api/platforms` carries an `events` block per platform:

```json
{ "declared": true, "connected": true,
  "last_heard": "2026-09-08T12:36:58Z", "last_ended": null,
  "endings": 0, "watchers": 1 }
```

Here rather than in the log, because a stream that has quietly died is
invisible from every other angle: the panels still draw, and they draw less
currently than anybody watching believes. An operator asking why a dashboard
feels behind has nowhere else to look.

`declared` and `connected` are reported separately and an operator should never
have to infer one from the other. A platform that offers no stream is behaving
exactly as intended; one that offers a stream the shell is not currently holding
is a different situation, and a platform whose stream nobody is watching is a
third — all three would read identically as a single boolean.

`endings` and `last_ended` are kept across reconnections, because the case worth
catching is a stream that is up now and keeps falling over. That looks perfectly
healthy at any single instant.

## Failure Modes

The list is the point of the document. Every row degrades to the polling
behaviour that exists today.

| What happens | What the shell does | What a viewer sees |
|---|---|---|
| Platform declares no `events` | Polls, as now | Nothing different |
| `events` path 404s or refuses | Logs once, retries with backoff, polls at `refresh_ms` | Nothing different |
| Stream connects, then drops | Reconnects with backoff; every panel back to `refresh_ms` in the meantime | Nothing different |
| Stream stays open but stops delivering | Missed heartbeat is treated as a drop: back to `refresh_ms`, reconnect | Nothing different |
| Stream connects and reports nothing, ever | Polls at the relaxed interval | Data up to the relaxed interval stale |
| Event names an unknown panel | Discards it | Nothing |
| Event names a panel nobody watches | Discards it, no fetch (REQ-2.2) | Nothing |
| Platform floods events | Coalesced, jittered, and clamped by the refresh floor | Nothing different |
| Event arrives, fetch then fails | The existing outcome table applies unchanged ([[HLIN-S-0003]]) | The state that table already specifies |
| Platform reports changes it did not make | Wasted requests, bounded by the floor | Nothing |
| Platform fails to report changes it did make | Caught by the relaxed interval | Data up to the relaxed interval stale |

## Conformance

**A platform's stream conforms if**: it is SSE at the declared path; it emits
`changed` events whose `data` is an object with a `panel` field naming a key in
its own manifest; it emits a comment-line heartbeat at least every 20 seconds;
it holds the connection open indefinitely and tolerates the shell reconnecting
at any time; and its panels remain correct to fetch whether or not any event was
ever sent.

**The shell conforms if**: it never renders data that did not come from a fetch
it issued; it polls a platform that declares no stream exactly as it did before
this document existed; it returns to `refresh_ms` for every panel of a platform
whose stream is not currently connected; and it holds no connection to a
platform none of whose panels are on a live surface.

**The suite must include** a platform that declares a stream and never sends
anything, one that sends events for panels that do not exist, one that closes
the stream mid-session, one that holds the connection open and goes quiet
without closing it, and one that floods — because each of those is a real
platform on a bad day, and the claim this document makes is that none of them
can make the shell worse than it is today. The fourth is the one worth writing
first: it is the only failure with no signal of its own.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0010]] | The shell polls platforms | decided | The floor this accelerates over |
| [[HLIN-A-0011]] | A platform may offer an event stream | decided | This specification is that decision in detail |
| [[HLIN-A-0009]] | A panel may declare how fresh its data is | decided | `refresh_ms` becomes the interval used when push is absent or down |

## Open Items

Settled 2026-09-08, and recorded as settled rather than removed so that the
reasoning survives the decision:

- **The relaxed interval** is `max(panel refresh_ms, shell refresh_ms)` — a rule
  rather than a number, so it needs no new setting and cannot drift from the
  operator's existing answer to the same question.
- **`selections` matching is by subset**, which makes the empty case fall out of
  the rule instead of needing one of its own.
- **Heartbeats are required**, at 20 seconds, because a silently dead connection
  is the only failure here with no signal of its own and the only way this
  feature could leave a viewer worse off than plain polling.

- **Stream health reaches the operator channel**, on `/api/platforms`. See
  "What an operator can see" above. Settled 2026-09-08 under [[HLIN-T-0048]],
  along with the connection sharing REQ-2.1 asks for — health had to live
  wherever the connection lives, and the connection belonged in one place
  rather than one per surface.

Nothing is currently open.
