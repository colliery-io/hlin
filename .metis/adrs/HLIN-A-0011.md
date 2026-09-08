---
id: 002-a-platform-may-offer-an-event
level: adr
title: "A platform may offer an event stream; the shell subscribes and keeps polling as a floor"
number: 2
short_code: "HLIN-A-0011"
created_at: 2026-09-08T10:50:00.000000+00:00
updated_at: 2026-09-08T10:50:00.000000+00:00
decision_date: 2026-09-08
decision_maker:
parent:
archived: false

tags:
  - "#adr"
  - "#phase/decided"

exit_criteria_met: false
initiative_id: NULL
---

# ADR-2: A platform may offer an event stream; the shell subscribes and keeps polling as a floor

## Context

[[HLIN-A-0010]] records that the shell pulls, and what that costs: 20 upstream
requests a second for one surface of four panels at `refresh_ms = 125`,
independent of how many people are watching. It also records the waste — those
requests are paid whether or not anything changed, and a cadence hint can only
ever be an average.

The residue is worth removing. What it must not cost is the three properties
that make the pull design work: a platform that needs no knowledge of the shell,
a shell that controls its own load, and failure that is local and legible.

Two questions decide the shape. They are independent, and conflating them is how
this kind of design goes wrong.

## Decision

### Who opens the connection: the shell

**A platform declares a stream endpoint in its manifest, and the shell opens a
connection to it.** The same direction as everything else the shell does — it
already fetches the manifest, the health endpoint and every panel's data from
paths a platform declares — so a platform stays a plain HTTP server that answers
when asked and needs no address for the shell, no credential to send it, and no
knowledge that Hlin exists.

The alternative, a platform POSTing to the shell, was rejected on that ground.
It inverts the dependency the product is built on: a platform would need to be
configured with the shell's address and authenticate to it, and the shell would
grow an inbound endpoint that anything on the network can try. Twelve platforms
that register against a shell would become twelve platforms that deliver to one.

A shared bus was rejected for making a message broker a hard dependency of a
system whose dependency list is Postgres and HTTP, in exchange for decoupling
that the manifest already provides.

### What is sent: the news, not the data

**An event says that a panel's data changed. It does not carry the data.** The
shell then fetches, exactly as it does today. Push replaces the *timer*, not the
fetch.

The reason is that a frame is not a property of a panel — it is a property of a
panel *and who is asking and what they selected*. The shell fans out on
precisely that: one surface per (layout, principal), parameters per panel
instance, a credential minted per request. A platform pushing a ready frame
would have to know every viewer and every selection to produce a correct one,
which is knowledge the design deliberately does not give it; the only frames it
could push unconditionally are those identical for everybody, which is a subset
it cannot identify on its own.

Sending only the news keeps every property that already holds. The data path,
the credentialing (HLIN-A-0008), the parameter allow-list, the envelope
validation, the per-principal deduplication (HLIN-A-0004) and every row of the
outcome table in [[HLIN-S-0003]] are unchanged, because the fetch they describe
still happens and is still the shell's. Nothing new can be true of a frame that
arrived by push, because no frame arrives by push.

It also makes a lost event cheap. Because polling continues underneath, a
notification that never arrives costs one interval of staleness rather than
correctness — which is what lets the shell treat delivery as best-effort and
skip every mechanism that guaranteed delivery would require.

Carrying payloads for panels a platform declares viewer-independent is a
coherent later optimisation and is deliberately not day one: it buys a round
trip on a subset of panels in exchange for a second data path with its own
validation and its own failure modes, and the round trip it saves is one the
dedup has already made cheap.

### Push accelerates; it never replaces

Polling continues for every panel, at a relaxed interval while its platform's
stream is connected and the panel is declared as pushed, and at its declared
cadence otherwise. A platform that offers no stream is unaffected. A stream that
drops takes the shell back to plain polling rather than to silence, and the
transition needs no recovery logic because the poll was never turned off.

This is the constraint HLIN-A-0010 set, and it is the whole reason the feature
can be added without re-examining how the shell degrades.

## Consequences

**A panel must say it is pushed.** Otherwise the shell cannot tell "no event yet
because nothing changed" from "this panel is never pushed", and would relax the
polling of a panel it will never hear about — trading redundant requests for
silent staleness, which is a worse deal than the one it started with.

**One event can start many fetches.** A panel watched by twenty principals with
different selections is twenty distinct fetches that the tick currently spreads
out and an event would fire at once. The specification has to say what the shell
does about that; a coalescing window with jitter is the obvious answer, and the
obligation is noted here so the design cannot quietly skip it.

**The shell holds one connection per platform**, not per surface or per viewer,
and it is held only while at least one surface is watching something that
platform offers. A shell watching nothing should hold nothing open.

**The event stream is not contract.** A platform adding, moving or withdrawing
it is not a breaking change and does not require a major version: the shell is
correct without it by construction. It is therefore excluded from the contract
hash, for the reason `kind` and `refresh_ms` are (HLIN-A-0002, HLIN-A-0009).

**A platform can lie about what changed** — over-report and cost the shell
requests it did not need, or under-report and leave panels as stale as the
relaxed poll allows. The first is bounded by the shell's own floor, which it
keeps. The second is bounded by the relaxed interval, which is why the relaxed
interval is a real interval and not infinity.

## Decision Log

| ADR | Relationship |
|-----|--------------|
| [[HLIN-A-0010]] | Sets the floor this accelerates over, and the three properties it must not cost |
| [[HLIN-A-0009]] | `refresh_ms` becomes the interval used when push is absent or down |
| [[HLIN-A-0004]] | Per-principal dedup is why a pushed frame could not be correct for everyone |
| [[HLIN-S-0003]] | The outcome table is untouched, because the fetch it describes still happens |
