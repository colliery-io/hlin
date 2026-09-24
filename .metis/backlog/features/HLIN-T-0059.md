---
id: a-platform-may-want-a-component
level: task
title: "A platform may want a component the shared library does not have"
short_code: "HLIN-T-0059"
created_at: 2026-09-24T20:26:07.019352+00:00
updated_at: 2026-09-24T22:23:33.291793+00:00
parent:
blocked_by: []
archived: false

tags:
  - "#task"
  - "#feature"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: NULL
---

# A platform may want a component the shared library does not have

## Objective

Decide how a platform gets a component that only it needs, one the shared
design library will never carry, without breaking the vision's constraint that
no code crosses the platform/shell boundary.

## Context

Raised by the user during design of [[HLIN-I-0010]]: "we do need to find a way
for people to customize components as well. It's entirely possible a service
would have a component that isn't shared by the entire library."

What exists today:

- `component` ([[HLIN-I-0004]]) lets a platform name a component a *pack*
  offers. It does not help when no pack offers it, and every pack is built
  centrally.
- The vision names the escape hatch: "a bounded declarative view spec ... not
  built until composition-level customization has demonstrably failed a real
  case", and forbids embedding a platform's own frontend.

So the candidates are, roughly: a declarative view spec (data, not code); a
pack extension mechanism that platform teams contribute to, which reintroduces
central release but not central design; or a revision of the vision's
constraint. The first is the one the vision already anticipates.

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

## Acceptance Criteria

- [x] A real case written down where `kind` + `component` cannot express what a
      platform needs
- [x] Options compared against the vision's constraints, with a recommendation
- [x] Either an initiative opened or a decision recorded not to

## Status Updates

### 2026-09-24

Recorded from [[HLIN-I-0010]] design. Not started.

### 2026-09-24 — "forward the rendering" as an option

The user asked whether code has to cross at all: could the platform render the
component and the shell forward the result? It depends on what is forwarded:

- **Rendered markup, no script.** The platform serves an HTML/SVG fragment
  beside its envelope. The shell sanitises it with an allowlist (no script, no
  event handlers, no external loads, styling only through design tokens and
  classes), inserts it, and wires interaction itself from attributes such as
  `data-hlin-action="toggle" data-row="i1"`, which dispatch declared actions
  ([[HLIN-A-0013]]). No code crosses. This is arguably the vision's
  "declarative view spec", with HTML as the declaration.
- **A live component (iframe, JS, WASM, SSR plus hydration).** Code crosses,
  and the browser talks to the platform directly, which breaks the auth model
  (identity lives in the shell, not in a platform session). The vision rules
  this out explicitly ("Embedding a platform's own frontend in a panel is not a
  supported mechanism").

The first keeps safety (sanitised, same origin, actions still named and
dispatched by the shell) and totality (the envelope stays the contract; bad
markup falls back to `kind`). What it gives up is the vision's central bet,
that design consistency is structural: a fragment is only as consistent as its
author's discipline with the tokens. It amends the vision's Customization
feature and the "shell renders everything" principle, so it is a vision-level
decision.

### 2026-09-24 — answered by micro UI

The user set the goal as micro UI, with a sandboxed iframe per panel
([[HLIN-A-0014]]). A platform ships whatever component it needs in its own
module, so this item is answered by that decision. Close it when A-0014 is
decided. Decided the same day; the work is [[HLIN-I-0011]].
