---
id: 001-a-panel-may-declare-how-fresh-its
level: adr
title: "A panel may declare how fresh its data is, as a hint the shell clamps"
number: 1
short_code: "HLIN-A-0009"
created_at: 2026-09-08T07:08:07.035754+00:00
updated_at: 2026-09-08T07:08:07.035754+00:00
decision_date:
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"


exit_criteria_met: false
initiative_id: NULL
---

# ADR-1: A panel may declare how fresh its data is, as a hint the shell clamps

## Context

The refresh interval is one number per shell. Until recently it was a constant
in the aggregator reachable from no configuration at all; it is now
`timings.refresh_ms`, and an operator sets it once for every panel from every
platform.

That number cannot be right for everything a shell composes. A queue depth
changes several times a second and a nightly batch count changes once a day, and
the shell asks both of them at whatever interval the operator picked — so the
choice is between missing the first and hammering the second. The demo makes it
concrete: `live-rate` needs `refresh_ms = 125` to be worth looking at, and at
that setting `records-per-second`, whose underlying wave has a fifteen-minute
period, is fetched two hundred and forty times more often than it changes.

The publisher knows the answer. It is their data, they know its cadence, and
`hlin-manifest::Panel` has no field for saying so.

The question this decision answers is not whether to add the field — it is what
the field *means*, because the manifest is a contract and its vocabularies only
ever grow (HLIN-A-0003). A field admitted with the wrong semantics cannot be
taken back.

## Decision

**A panel may declare `refresh_ms`. It is a non-contract hint, and the shell's
own setting bounds it.**

Three parts, each load-bearing:

**Non-contract.** `refresh_ms` is excluded from the contract hash and from the
diff classification, exactly as `kind` is and for the same reason (HLIN-A-0003):
it is what the platform *suggests*, not what it promises. A platform that
discovers its data is slower than it thought should be able to say so on a
Tuesday afternoon without a major version bump and without the shell raising a
violation. Nothing a consumer can pin to has moved.

**A hint, not an instruction.** The shell honours it per instance, clamped by
its own configuration. A platform declaring `refresh_ms = 1` does not get polled
a thousand times a second: it gets the shell's floor. The shell is the one
paying for the requests, running the surfaces, and answering for the load on
every other platform it fronts, so the shell keeps the last word. A platform
that wants to be asked less often is always obeyed; one that wants to be asked
more often is obeyed up to the operator's ceiling.

**Staleness follows it.** A panel refreshed every 125ms should not be called
current for ninety seconds. Where a panel declares its own cadence, its
staleness threshold is derived from that cadence rather than from the shell's,
by the same proportion the shell already uses — so "this data is old" keeps
meaning the same thing across panels that move at different speeds.

A panel that declares nothing behaves exactly as it does today. This is
additive in the strict sense: no existing manifest changes meaning.

## Alternatives Analysis

| Option | Pros | Cons | Risk Level | Implementation Cost |
|--------|------|------|------------|-------------------|
| Non-contract hint, shell clamps (chosen) | Cadence travels with the panel that knows it; no version bump to tune it; the shell keeps control of its own load | Two numbers instead of one; an operator debugging "why is this slow" has to look at the manifest as well as the config | Low | Low |
| Contract, and binding | A viewer gets a guarantee about freshness; drift is caught by the machinery that already exists | Makes routine tuning a major version bump, which is exactly the friction HLIN-A-0002's debounce exists to avoid; and a platform could compel a shell to poll it arbitrarily hard | Medium | Medium |
| Leave it per shell | Nothing new in the manifest; simplest | The wrong number for every panel that is not the median; the demo already needs two configurations to show both kinds of data | Low | None |
| Per-panel *operator* override in shell config | Shell keeps full control; no manifest change | The operator has to know the cadence of every panel of every platform, which is exactly the knowledge the publisher has and they do not | Medium | Medium |

## Consequences

**What gets better.** `demo/hlin-live.toml` stops being necessary to see live
data: the panels that move fast say so, and one demo shows both. An operator
sets a bound rather than a value, which is a decision they can actually make.

**What to watch.** Two places now influence how often a platform is asked, and
an operator diagnosing load has to look at both. The shell should say which one
is in force when it matters — the clamp is the interesting case, because a
platform asking for something it is not getting is a disagreement worth
surfacing.

**What this does not open.** It is not a step toward platforms controlling shell
behaviour generally. The reason this one is safe is that the shell clamps it and
pays nothing it did not agree to pay; a hint without that property would be a
different decision.

## Decision Log

- **2026-09-08**: Decided. Chosen over a binding contract field because tuning a
  cadence is not breakage and should not need a major bump, and over leaving it
  per shell because the publisher is the one who knows.
