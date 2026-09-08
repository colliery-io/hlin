---
id: chrome-tokens-the-shell-asks-the
level: task
title: "Chrome tokens: the shell asks the pack for its colours"
short_code: "HLIN-T-0021"
created_at: 2026-09-07T22:22:00+00:00
updated_at: 2026-09-07T23:37:07.152189+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# Chrome tokens: the shell asks the pack for its colours

## Parent Initiative

[[HLIN-I-0003]]

## Objective

The bar, the picker and the grid are Hlin's own product and Hlin styles them,
today with a hardcoded light palette. A dark pack inside a light shell is
neither. Hlin should declare the handful of colours its chrome needs and let a
pack fill them, without ever learning that pack's token names.

## Acceptance Criteria

## Acceptance Criteria


- [x] `app.css` uses custom properties for every colour, with defaults that make the shell usable with a pack that supplies none
- [x] The property names are Hlin's own and describe roles rather than hues: surface, raised surface, border, text, dimmed text, accent
- [x] The set is small enough to be a contract somebody would fill, and documented where a pack author would look
- [x] A pack fills them by mapping its own tokens, and the demo pack does so
- [x] Not one hex value left in `app.css` outside the default block
- [x] Checks clean, the browser suite passing, the shell still legible with the demo pack

## Implementation Notes

### Technical Approach
Cascading custom properties do the work: the pack's stylesheet sets
`--hlin-surface: var(--panel)` and the chrome reads `var(--hlin-surface)`.
Hlin's defaults live on `:root` in `app.css` so the shell stands alone; a pack
overrides them by shipping later in the cascade, which is already the order the
page loads them in.

Keeping the set small is the whole discipline. Every property added is one more
thing a design system has to answer for, and a chrome that needs twenty is a
chrome that should be the pack's to draw.

### Dependencies
[[HLIN-T-0020]] delivers the stylesheet, which is where a pack sets them.

### Risk Considerations
The temptation is to grow this into a theming system. It is not one. It is the
minimum that stops a dark pack sitting in a light frame, and if it starts
covering spacing, radii and typography then the honest answer is that the chrome
belongs to the pack.

## Status Updates

### 2026-09-07 — done, eleven properties

Seventeen distinct hex values in the chrome became eleven properties named for
what they do. Nothing outside the defaults block is a colour any more.

| Role | For |
|---|---|
| `--hlin-surface` | The page behind everything |
| `--hlin-raised` | Bars, panels, inputs |
| `--hlin-border` | Every line between two things |
| `--hlin-text` | Ordinary reading text |
| `--hlin-dim` | Labels and captions |
| `--hlin-faint` | Affordances at rest, like an unheld drag handle |
| `--hlin-accent` | The one colour meaning "this, here" |
| `--hlin-on-accent` | Text legible on top of the accent |
| `--hlin-good` / `--hlin-warn` / `--hlin-bad` | Connected, waiting, wrong |

Eleven rather than the six this task sketched. The three state colours earned
their place because the shell genuinely reports those states about itself: the
stream is live, a parameter change is applying, something went wrong. `faint`
against `dim` earned its place too, because a drag handle at rest and a caption
are different weights and every design system has both.

**Tints come from `color-mix` rather than more properties.** Hover backgrounds
and the trouble banner were four more hex values; they are now a percentage of
an existing role. That kept the contract at eleven instead of fifteen, and it
means a pack that changes its accent gets the matching hover for free.

**The defaults are load-bearing, not decoration.** A pack that fills nothing
still gets a legible shell, which matters because the first thing anybody writes
is a pack with ten drawing methods and no chrome mapping. It should look
unfinished, not broken.

**A test caught the change, correctly.** The demo pack has a test asserting
every rule in its stylesheet is scoped under one class, so it cannot reach into
a host page. Filling `:root` properties is deliberately unscoped, which is the
mechanism rather than a leak, and the test failed exactly as it should have.
Rather than loosening it, it now allows one `:root` block and asserts every
declaration inside it is a `--hlin-` property, so the exception cannot smuggle
in real styling. It also asserts the pack actually fills them, because a test
that passed without the exception being exercised would not be checking it.

**The screenshots are identical to before.** That is the result: the demo pack
fills the properties with the values that were hardcoded, so tokenising changed
nothing visible. What it changed is that a dark pack can now change all of it.

Checks clean, 269 Rust tests passing, 15 browser tests passing.
