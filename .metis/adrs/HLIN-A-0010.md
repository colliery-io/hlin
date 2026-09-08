---
id: 001-the-shell-polls-platforms-a
level: adr
title: "The shell polls platforms; a platform never pushes into it uninvited"
number: 1
short_code: "HLIN-A-0010"
created_at: 2026-09-08T10:47:36.290306+00:00
updated_at: 2026-09-08T10:47:36.290306+00:00
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

# ADR-1: The shell polls platforms; a platform never pushes into it uninvited

## Context

Every panel on every surface is kept current by the shell asking for it. The
aggregator decides what is due, the driver fetches it, and a frame goes out over
the browser stream. That has been true since the first line of the stream code
and it has never been written down as a decision — which is why it is being
written down now, before the question of push is taken, rather than after.

**What it costs, measured.** One surface of four panels declaring
`refresh_ms = 125`, on the demo, against two sample platforms:

| viewers of that surface | upstream requests per second |
|---|---|
| 1 | 20 |
| 3 | 20 |

The second row is the whole reason the design survives. Cost scales with
distinct *(principal, panel, parameters)* and not with viewers, because the
shell deduplicates per principal (HLIN-A-0004, refined 2026-09-08) and holds one
surface per (layout, principal) that every viewer of it subscribes to. Ten
people watching one wallboard cost what one person costs.

**What it wastes.** The requests are paid continuously whether or not anything
changed. HLIN-A-0009 reduced this from one interval per shell to one per panel,
which is as far as a cadence *hint* can go: a publisher can say how often their
data usually changes, and cannot say when it actually did. A panel polled at its
honest average still fetches identical bytes most of the time, and a panel whose
data arrives in bursts is either late or over-fetched, never both right.

That residue is real and is the case for push. It is not the case for replacing
pull.

## Decision

**The shell pulls. A platform is a plain HTTP server that answers when asked,
and nothing a platform sends arrives at the shell unrequested.**

Three properties follow, and they are the ones worth protecting:

**A platform needs no knowledge of the shell.** It does not connect out, hold a
socket, discover an address, authenticate to anything, or know that Hlin exists
beyond serving a manifest at a well-known path. This is the whole premise of the
product — twelve independently released platforms that register against a shell
rather than integrate with it — and the moment a platform must *deliver* to the
shell, the shell is a dependency of every platform rather than the other way
round.

**The shell controls its own load.** It knows how many surfaces are live, how
many panels each holds, and it sets the floor on cadence (HLIN-A-0009). A push
sender decides for it, and a platform having a bad day becomes the shell having
a bad day.

**Failure is legible and local.** A fetch that does not answer is one panel that
says so, on a retry backoff, on one surface. There is no delivery state to
reconcile, no queue to drain, no question of what a viewer missed while
disconnected: the next poll is the whole recovery mechanism, and every panel is
one poll away from correct however badly the previous one went.

The browser leg is the opposite and deliberately so: the shell pushes to the
browser over SSE (HLIN-S-0003). The asymmetry is not an inconsistency. The shell
knows every browser watching it because each one is holding a connection it
opened, and a browser that misses frames reconnects and is resent state. Neither
is true of a platform, and neither is something a platform should have to make
true.

## Consequences

**Accepted.** Data is at most one cadence-interval stale, and requests are made
that return nothing new. On the demo's fastest panels that is 20 requests a
second per surface, most of them redundant.

**Accepted.** A panel whose data changes in bursts cannot be both timely and
cheap. `refresh_ms` picks a point on that trade and the publisher picks the
number, which is better than the shell guessing and worse than knowing.

**Constrained.** Any future push mechanism is an *accelerator over this floor*,
not a replacement for it. Concretely: a platform that offers push must remain
correct when nobody consumes it, the shell must keep working against a platform
that offers none, and a push that stops arriving must degrade to polling rather
than to silence. A design in which the shell is only correct while a delivery
channel is healthy would give up all three properties above to save requests
that dedup has already made cheap.

**Open.** Whether push carries data or only the news that data changed is the
next decision and is deliberately not taken here. See the initiative that
follows this ADR.

## Decision Log

| ADR | Relationship |
|-----|--------------|
| [[HLIN-A-0009]] | The cadence hint is how far a publisher can tune polling without changing its shape |
| [[HLIN-A-0004]] | Per-principal dedup is what keeps the cost independent of viewers |
| [[HLIN-S-0003]] | The shell→browser leg pushes, for reasons that do not transfer to the platform→shell leg |
