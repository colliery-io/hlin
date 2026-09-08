---
id: the-demo-rendered-by-aurora-dark
level: task
title: "The demo, rendered by Aurora Dark"
short_code: "HLIN-T-0024"
created_at: 2026-09-07T22:25:00.000000+00:00
updated_at: 2026-09-07T22:25:00.000000+00:00
parent: HLIN-I-0003
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0003
---

# The demo, rendered by Aurora Dark

## Parent Initiative

[[HLIN-I-0003]]

## Objective

A front end composed through Hlin and drawn by Aurora Dark, and the three
measures checked. This is where the initiative either holds or does not.

## Acceptance Criteria

- [x] A frontend binary under `examples/`, depending on `hlin-ui` and on vendored Aurora with the `hlin` feature, mounting the app with Aurora's pack. **Inside the workspace, not outside as this criterion said**: the measure is on `crates/`, and members share a lock file and are built by `cargo check --workspace`, which is worth more than the symbolism of exclusion. `vendor/aurora-leptos` is the thing that is excluded, because linting somebody else's source is not ours to do
- [x] An angreal task building it, and the demo able to serve it instead of the demo-pack frontend
- [x] **Measure one:** Aurora's default build unaffected by the feature, shown by building it with the feature off and diffing the resolved dependency set
- [x] **Measure two:** a grep for "aurora" across `crates/` returning nothing
- [x] **Measure three:** `angreal demo up` renders every panel through Aurora, and the browser suite passes against it with selectors updated to Aurora's markup
- [x] Screenshots taken and looked at, and what they show written down
- [x] The walkthrough still passing, since none of this should have touched the shell's behaviour

## Implementation Notes

### Technical Approach
The binary should be small enough to read in one sitting. If it is not, the
generic work in [[HLIN-T-0019]] did not go far enough.

The browser suite asserts on the demo pack's class names in a few places and
will break. That is the suite doing its job. Assertions on `data-state` are the
shell's and should survive untouched; if they do not, something is being asked
of the pack that is not its business.

### Dependencies
Everything: [[HLIN-T-0019]] through [[HLIN-T-0023]].

### Risk Considerations
Two frontends now exist, one per pack, and they can drift. The demo-pack one
earns its place by depending on nothing published; if keeping both in step
becomes work, the answer is that the example is the real one and the demo-pack
frontend is a smoke test.

## Status Updates

### 2026-09-07 — all three measures hold

`examples/frontend-aurora` is sixteen lines, and thirteen of them are a doc
comment. The difference from `frontend-demo` is one identifier and one
dependency. That was the claim the generic work in [[HLIN-T-0019]] existed to
make, and it is now checkable by reading two files side by side.

**Measure one: Aurora's default build is unaffected.** Measured from Aurora's
own directory, because the workspace resolves features across every member and
would otherwise flatter the answer — `frontend-aurora` enables `hlin`, so a
workspace-wide `cargo tree` shows it on for everybody.

| | Direct dependencies |
|---|---|
| Default | `js-sys`, `leptos`, `web-sys` |
| `--features hlin` | those, plus `hlin-view` and `serde_json` |

**Measure two: `grep -ril aurora crates/` returns nothing.**

**Measure three: fifteen browser tests pass against Aurora**, and the same
fifteen still pass against the demo pack. The walkthrough passes, exit 0, so
none of this touched the shell's behaviour.

**Three assertions were pack-specific and are now not.** `.value`, `.spark` and
`.reason` are the demo pack's class names, and asserting on them ties the suite
to one design system — the thing Hlin exists to make swappable. They are
replaced by `drewSomething`, which asks whether a panel has content below its
heading whatever a pack put there, and by comparing rendered HTML before and
after a kind switch rather than naming what a sparkline looks like. Assertions
on `data-state` were untouched, because that is the shell's and survives any
pack, which is what the task predicted.

**One of those replacements was wrong, and the demo pack caught it.**
`drewSomething` is a plain `evaluate`, so unlike every Playwright assertion it
does not retry. It passed against Aurora and failed consistently against the
demo pack, for a reason worth keeping: Aurora's skeleton contains the words
"loading sparkline" and the demo pack's is empty grey bars, so a one-shot check
during the refetch after a write saw content in one and nothing in the other.
Now polled. Two packs disagreeing is exactly what a second pack is for.

**The blanking after every write is now visible in two packs.** Changing a kind
writes the layout, which drops the surface, which blanks every panel until the
refetch lands. It is correct, it is not free, and it is recorded in
[[HLIN-T-0017]] as work worth doing when somebody arranges a surface for more
than a minute at a time.

**What Aurora looks like.** Stats with tabular figures and coloured delta pills,
a timeseries in Aurora's accent wheel with a dotted legend, a health rollup as a
`HealthPill` with per-worker dots, a mono table. The bar, picker and grid are
dark alongside the panels, because the chrome properties from [[HLIN-T-0021]]
are answered by Aurora's own tokens. Edit mode is dark throughout: drag handles,
kind selects, resize corners.

Checks clean, 269 Rust tests passing, 15 browser tests passing against each
pack, walkthrough passing.
