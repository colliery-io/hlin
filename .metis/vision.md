---
id: hlin
level: vision
title: "hlin"
short_code: "HLIN-V-0001"
created_at: 2026-08-04T05:02:44.175681+00:00
updated_at: 2026-08-04T14:08:51.939819+00:00
archived: false

tags:
  - "#vision"
  - "#phase/published"


exit_criteria_met: false
initiative_id: NULL
---

# Hlin Vision

**One place to work across many systems, assembled by the people who use them.**

Hlin is named for the Norse goddess who watches over those she is named to protect.

## Purpose

We are building roughly a dozen Rust platforms. Each has a Leptos frontend served from its own root, each ships on its own cadence, and each is developed by a team that should not have to coordinate with the other eleven to release.

The cost lands on the people using them. Their work is spread across a dozen origins, so anything that spans two systems means holding two tabs, signing in twice, and doing the correlation by hand. There is no single place to see how things are, and no single place to act on it. Each frontend is coherent on its own; together they are twelve tools rather than one product.

The obvious fixes are both wrong. Building a thirteenth hand-written application puts every cross-cutting need behind a central team's backlog and goes stale the moment a platform changes. Composing the frontends naively, as a dozen WASM binaries sharing one document and one authority, makes every platform's bug everyone's outage and puts every platform's credentials in the browser.

## Product/Solution Overview

Hlin is a workspace shell.

Each platform declares, at runtime, what it offers, and ships its own UI for it as modules. Hlin discovers those declarations, runs each module in its own sandbox, and lets a person select panels and pages from any platform and arrange them into a surface they authored. Where a platform offers data rather than a module, Hlin draws it itself.

Platforms own their data, their rules and their UI. Hlin owns the page, the person and the wire: the only code in the shell's page, the signed-in identity, and every request between a module and its platform.

That split produces most of the properties we want. A platform ships UI at its own pace, with no ceiling on what it can do. A module failing is one panel in a known state, never a broken page. The browser never holds a credential, because every request goes through the shell, which binds the viewer's identity to it and lets the platform decide. Adding a platform costs the shell nothing, because the shell was never compiled against it.

### What Hlin is not

**Not a reverse proxy.** The shell carries requests between modules and platforms, and that plumbing is necessary, but it is not the product. It carries only what a platform declared, only for the platform that owns the module asking.

**Micro-frontends, isolated.** Independently built modules, each in its own sandboxed frame with an opaque origin, reaching nothing but their own platform, and only through the shell. Never in the shell's page.

**Not a query layer.** Hlin shows what platforms choose to expose. It does not reach into anyone's database, and it defines no query language over platform internals.

**Not a general dashboarding tool.** Grafana exists and is better at being Grafana. Hlin hosts the UI of the teams that own the systems, in the terms of those systems.

**Not a central bottleneck.** Shipping a panel or a module does not require a Hlin release, a Hlin PR, or a design review.

**Not an authorization system.** Hlin knows who a person is. What they may do is each platform's decision, made from the identity the shell forwards and whatever the platform keeps about them.

## Current State

A dozen independent Rust platforms, each with its own Leptos frontend at its own origin, each on its own release cadence. Cross-system work is done by hand across browser tabs. No shared place to work exists, and no mechanism exists for one to emerge without central coordination.

## Future State

One shell where every platform's UI runs side by side, each sandboxed, and where the people using the systems, not a central team, compose the surfaces they work in. A person signs in once and acts on any platform as themselves. Adding a platform, a module, or a navigation entry is a deploy of that platform alone; the shell is never rebuilt to accommodate a child.

## Major Features

- **The manifest.** Each platform serves a document describing itself at a well-known path: navigation entries, offered panels and pages, their modules or data endpoints, the parameters each responds to, the route prefixes the shell may carry requests to, health and summary endpoints, and lifecycle status for anything deprecated. Three rules keep it survivable across a dozen independent release cadences: the schema version is monotonic and additive with unknown fields ignored; icons and view kinds are names from a shared vocabulary rather than code; a malformed or absent manifest degrades to a plain link with a warning indicator, never a shell error.
- **Discovery.** Hlin reads a platform list from runtime configuration and polls each manifest. The registry sits behind a trait, so a push-based model with heartbeat expiry, or orchestrator-native label discovery, can replace configuration later without reshaping anything above it.
- **Modules.** A platform's own UI for a panel or a page, built with the shared kit and the shell's SDK, served through the shell and run in a sandboxed frame. It talks to the shell over a versioned bridge: context in (time range, parameters, theme, who is signed in), requests and intents out. Decision HLIN-A-0014.
- **Identity and requests.** A person signs in to Hlin once. Every request a module makes goes through the shell, which binds the viewer's identity to it, and the platform decides what to allow. Decisions HLIN-A-0004 and HLIN-A-0013.
- **Panels drawn by the shell.** A panel may instead declare a view kind and a data endpoint returning a typed envelope, and the shell draws it. This is the way to show cross-platform summaries and time-driven charts, and the fallback when a module cannot load. Fan-out is aggregated shell-side, deduplicated across panels requesting the same data, and delivered over a single stream.
- **Shared context.** Shell-level controls, notably time range, drive every panel and module that declares the corresponding parameter, whoever drew it.
- **Views.** The view registry for shell-drawn panels lives in a companion crate beside the design system, so the component library stays free of Hlin's vocabulary.
- **Customization.** Users choose panels and pages, arrange them, and set titles and time ranges. A platform that needs something bespoke ships it in its own module; nobody waits on a vocabulary change.

## Success Criteria

A team ships a new module on Tuesday morning. Someone on another team has it on their surface Tuesday afternoon, next to panels from two other platforms, with no Hlin release, no design review, and no conversation between the two teams.

A person does a day's work in a platform without opening its own frontend.

The bet: that sandboxed modules stay light and consistent enough to feel like one product. Each panel on screen costs a document and a runtime, and consistency now rests on a shared kit and convention rather than on the shell drawing everything. If surfaces load slowly, or teams' UIs visibly drift apart, the answer is shared caching and a stricter kit, not loosening the sandbox. Watch both in the first two adoptions.

## Principles

**Platforms ship their UI; the shell hosts it.** A platform decides what its UI looks like and does. Consistency comes from a shared kit every module builds with and theme tokens the shell sends in.

**No platform code in the shell's page.** Every module runs in a sandboxed frame with an opaque origin. It cannot reach the shell's page, its cookies, another module, or the network.

**Every request goes through the shell.** A module holds no credential. The shell carries its requests, only to its own platform, only under paths that platform declared, with the viewer's identity bound to each one.

**The platform decides who may do what.** Hlin authenticates and forwards; it holds no roles and interprets nobody's policy.

**Platforms are autonomous.** Adding a platform, a panel, a module, or a navigation entry is a deploy of that platform. Discovery happens at runtime against a manifest each platform serves about itself. Hlin is never rebuilt to accommodate a child.

**The manifest and the bridge are public APIs.** Panel keys, pages, parameters, route prefixes and the bridge protocol are a declared contract, not an implementation detail. They are versioned and deprecated with a window and a named successor. Removing a panel key without a major version is a breaking change; the shell detects it at runtime and flags it, and no build anywhere has to fail for the contract to be enforced.

**Vocabulary is additive and governed.** The view kinds the shell draws are a bounded set owned by the design system. Unknown kinds degrade to a fallback; they never break a layout.

**Composition belongs to users.** Selecting and arranging panels requires a deploy from nobody. The set of useful surfaces is not knowable in advance by any central team, so we do not try to enumerate it.

**Rendering is total over its inputs.** Every panel is in exactly one state at all times: loading, ready, stale, or unavailable, with unavailability distinguishing unreachable from malformed from unknown from deprecated. The shell draws the frame and state around every panel, so there is nothing a platform or a module can do, including never answering, that the shell has no rendering for.

## Constraints

- No platform code runs in the shell's page, in any phase. Modules run only in sandboxed frames.
- A module reaches nothing but its own platform, and only through the shell.
- Panels cross the platform/shell boundary as a module, as data plus a declared view kind, or both.
- The shell must never be rebuilt or released to accommodate a platform change.
- Unknown view kinds, failed modules and malformed manifests degrade gracefully; they never break a layout or produce a shell error.

## Design Questions, Answered

All five are now decided. Each names the decision that closed it.

- **The manifest schema.** A document at a well-known path, with contract identity separated from presentation, so a fingerprint moves only when a promise moves. Specification HLIN-S-0001.
- **The data envelope per view kind, and the initial vocabulary.** Five envelopes and six kinds, named separately and paired by an acceptance matrix, so data shape and rendering evolve on their own cadences. Specification HLIN-S-0002, decision HLIN-A-0003.
- **Where the view registry lives.** A companion crate, `hlin-view`, so the design system stays a plain component library that platform frontends can use without taking on Hlin's contract types. Decision HLIN-A-0005.
- **Layout persistence and sharing.** One owner per layout, personal or published to a shell-wide gallery, shared read-only with fork to edit. A layout referencing a platform the viewer cannot reach renders normally, with the refused panels showing as forbidden. Decision HLIN-A-0007.
- **Authorization.** Authentication is hoisted to Hlin, the shell forwards a signed identity on every request, and platforms decide at fetch time. The shell interprets nobody's policy. Decision HLIN-A-0004, specification HLIN-S-0004. Writes carry identity bound to the request. Decision HLIN-A-0013.

What remains open is recorded in those specifications rather than here.

## Crates

```
hlin              shell binary
hlin-manifest     contract crate: schema, panel entries, lifecycle, envelopes
hlin-view         view registry: kinds, the kind/envelope acceptance matrix, rendering
```

## Amendments

**2026-09-07.** Two sentences were corrected once the top-down design initiative ([[HLIN-I-0001]]) reached decisions that contradicted them.

- *Enforcement.* The principle "The manifest is a public API" said a breaking change "fails the build that attempts it". There is no build to fail: enforcement is runtime, in the shell, which is what keeps a dozen independently released platforms free of a shared gate ([[HLIN-A-0002]]).
- *The view registry.* Architecture said the registry lives in the design system. It lives in a companion crate beside it, so the twelve platform frontends that use the design system for their own purposes never take on Hlin's contract types ([[HLIN-A-0005]]).

The "Open design questions" section became "Design Questions, Answered" in the same pass; all five now have decisions.

**2026-09-24.** The central bet changed. Designing a signed-in, two-way demo ([[HLIN-I-0010]]) raised what Hlin is for, and the owner chose: the place people work rather than a vantage over systems; platforms, not the shell, decide how their UI looks; only our own platforms; and micro UI, each module in a sandboxed iframe ([[HLIN-A-0014]]).

- *Purpose and tagline.* "One vantage over many systems" became "one place to work across many systems". Hlin is a workspace shell, not a composition shell for views.
- *Rendering.* "The shell renders everything" is replaced by "platforms ship their UI; the shell hosts it". Consistency moves from structural to a shared kit and convention. The shell still draws panels from data, as the way to show cross-platform summaries and as the fallback when a module fails.
- *Micro-frontends.* "Not micro-frontends, not now, not as a later phase" is reversed. The reasons behind it were kept as requirements rather than dropped: no platform code in the shell's page, a sandbox per module, no credential in the browser, every request through the shell.
- *New principles.* Every request goes through the shell with identity bound to it, and the platform decides who may do what ([[HLIN-A-0013]]).
- *The bet.* It was that a shared vocabulary costs less than coordination. It is now that sandboxed modules stay light and consistent enough to feel like one product.

What this leaves in place: autonomy, runtime discovery, the manifest as a contract, composition by users, and total rendering.
