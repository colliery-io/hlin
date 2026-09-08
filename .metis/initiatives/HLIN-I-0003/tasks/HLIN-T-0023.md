---
id: vendor-aurora-dark-and-give-it-a
level: task
title: "Vendor Aurora Dark and give it a hlin feature"
short_code: "HLIN-T-0023"
created_at: 2026-09-07T22:24:00+00:00
updated_at: 2026-09-08T00:11:46.788117+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# Vendor Aurora Dark and give it a hlin feature

## Parent Initiative

[[HLIN-I-0003]]

## Objective

Aurora becomes a design pack by implementing Hlin's types behind a feature that
is off by default. Vendored into this repository and depended on by path, so the
demo builds from a clean clone and the ergonomics can be felt before anything is
published.

This is the task that answers the question the initiative exists to ask: is the
seam somewhere a design system can reach?

## Acceptance Criteria

## Acceptance Criteria


- [x] `aurora-leptos` vendored under `vendor/`, with a note recording where it came from, at what revision, and that the feature written here must go back upstream
- [x] A `hlin` feature, off by default, adding an optional `hlin-view` dependency and nothing else
- [x] Aurora's default build unchanged: with the feature off, no new dependency and no new compile cost
- [x] `DesignPack` implemented for an Aurora pack type, all ten methods, using Aurora components through its own `PATTERNS.md` guidance
- [x] Tokens throughout and not one hardcoded hex, per Aurora's own rule
- [x] `stylesheet` returning `AURORA_CSS` plus the mapping onto Hlin's chrome properties
- [x] `timeseries` and `sparkline` hand-drawn in SVG using Aurora tokens, since Aurora has no chart
- [x] The `Context` honoured: an aged panel visibly aged with its age, a dimmed one carrying its reason, and the party that is unreachable never named by the pack
- [x] A record of what was awkward, which is the finding this task exists to produce

## Implementation Notes

### Technical Approach
Follow Aurora's guidance rather than inventing markup: `Panel`, `Table`, `Stack`,
`Group`, `Text`, `Pill`, `Dot`, `HealthPill`, `Loading`, `Empty`, `Alert`,
`List`. Aurora's components take `Children` as boxed closures, so envelope
fields must be hoisted into owned locals before a `view!` or the borrow escapes.

Two of the ten methods are most of the work, because Aurora has no plotting
primitive. Draw both from one scaling pass so lines share an axis, and break the
path at a gap so a missing reading is not drawn as a smooth number.

### Dependencies
[[HLIN-T-0019]] through [[HLIN-T-0022]]. The interface has to be final and the
crates have to be shaped for an outside consumer before this is worth writing.

### Risk Considerations
The vendored copy is a staging area and will diverge from `aurora-dark` the
moment it is edited. That is acceptable while ergonomics are being worked out
and unacceptable once they are; the note in `vendor/` is what keeps it honest.

The real risk is discovering the seam is awkward. That is not a failure of this
task, it is its output, and it should be written down rather than worked around
quietly.

## Status Updates

### 2026-09-07 — Aurora draws Hlin's panels, and the seam held

`vendor/aurora-leptos` at `cb84a23`, with one addition: a `hlin` feature, off by
default, adding an optional `hlin-view` dependency and `src/hlin.rs`. Aurora's
default build is untouched, measured from its own directory so workspace feature
unification cannot flatter the answer:

| | Direct dependencies |
|---|---|
| Default | `js-sys`, `leptos`, `web-sys` |
| `--features hlin` | those, plus `hlin-view` and `serde_json` |

**One ergonomics defect, found by writing the pack.** `hlin-view` did not
re-export the envelope types, so implementing `DesignPack` needed a dependency
on `hlin-manifest` as well — for types that appear in the trait's own method
signatures. Fixed by re-exporting them, which also stops a pack compiling
against a different version of the contract than the vocabulary it implements.
Exactly the sort of thing only an outside implementor hits, which is why this
task exists.

**What was awkward, which is the finding this task was for:**

- **Aurora's components take children as boxed closures**, so a borrow of the
  envelope captured inside a `view!` has to outlive the call. Every field is
  pulled into an owned local first. It reads fine once you know, and it is the
  single most likely thing to trip up the next pack author.
- **Aurora has no chart.** Two of the ten methods are hand-drawn SVG, and they
  are most of the work in the module. Every future pack pays this again unless
  someone puts a plotting primitive somewhere shareable.
- **`clock` is written by hand** rather than pulling `chrono` in. A design system
  should not gain a date library to format one table cell.
- **Everything else was one component.** `stat` is a `Stack` and a `Pill`,
  `status` is a `HealthPill` and `Dot`s, `table` is `Table`, `skeleton` is
  `Loading`, `placeholder` is `Alert`. Aurora's `PATTERNS.md` pick-by-intent
  table answered every question without reading the source.

**Clippy had to stop at the vendor boundary.** Making the vendored crate a
workspace member meant linting somebody else's code, and it failed on Aurora's
own `widgets.rs` under this project's warning policy. The right answer is not to
edit upstream source to satisfy our lints, so `vendor/aurora-leptos` is excluded
from the workspace. A path dependency still resolves it.

**The chrome mapping works.** Aurora's `hlin.css` answers the eleven properties
from [[HLIN-T-0021]] with `var(--panel)`, `var(--ice)` and the rest, so the bar,
picker and grid are dark alongside the panels rather than a light frame around
them.

Seen in a browser: stats with tabular figures and coloured delta pills, a
timeseries in Aurora's accent wheel with a dotted legend, a health rollup as a
`HealthPill` with per-worker dots, and a mono table. Edit mode is dark
throughout.
