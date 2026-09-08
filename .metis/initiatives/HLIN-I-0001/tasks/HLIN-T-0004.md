---
id: hlin-view-registry-skeleton
level: task
title: "hlin-view: registry skeleton, acceptance matrix, state machine and totality test"
short_code: "HLIN-T-0004"
created_at: 2026-09-07T12:13:56.267271+00:00
updated_at: 2026-09-07T13:10:28.079713+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# hlin-view: registry skeleton, acceptance matrix, state machine and totality test

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Stand up `hlin-view` as the owner of the view vocabulary per [[HLIN-A-0005]]: the kind registry, the kind→envelope acceptance matrix, the panel state machine types, the `raw` fallback, and the totality test that proves every (kind, state, accepted envelope) triple has a rendering. No real rendering yet; the design system crate does not exist, so rendering resolves to a descriptor.

## Acceptance Criteria

## Acceptance Criteria

- [x] `Kind` enum with the six v1 kinds; `Kind::resolve(name) -> Kind` returns `Raw` for any unknown name (REQ-1.4)
- [x] `accepts(kind, envelope_name) -> bool` matches the spec's acceptance table exactly, including `raw` accepting everything and `table` accepting `series.v1`
- [x] `PanelState` enum: `Loading`, `Ready`, `Stale`, `Unavailable(Cause)` with `Cause` = `Unreachable | Malformed | Unknown | Deprecated | Forbidden`
- [x] A `plan(state, kind, envelope: Option<&Envelope>) -> RenderPlan` function that is total: returns a descriptor naming the effective kind, the state treatment and the notice, never panics, never returns an error. Named `plan` rather than `render` because it decides what to draw rather than drawing it
- [x] Totality test: iterates every kind × every state × every envelope the kind accepts (plus `None` for states without data) and asserts a plan is produced
- [x] Test asserting every v1 *panel* envelope is accepted by at least one non-`raw` kind (governance rule), and that `raw` accepts every envelope regardless of role. See the finding below on `options.v1`
- [x] Crate docs state that `RenderPlan` is the seam where the design system's components attach, and that the design-system dependency and version pin are added when the crate is available

## Implementation Notes

### Technical Approach
Keep the registry data-driven (a static table) so the totality test reads the same table the runtime uses. The state-transition rules from the spec are documented but not implemented here; transitions belong to the aggregator.

### Dependencies
[[HLIN-T-0001]], [[HLIN-T-0003]] (envelope types).

### Risk Considerations
Temptation to sketch real Leptos components. Resist: the design system is external and version-paired; anything drawn here now would be thrown away.

## Status Updates

**2026-09-07 — complete.** `hlin-view` now holds the rendering vocabulary in three modules: `kind` (the six kinds and the acceptance matrix), `state` (the eight panel states), and `render` (the total `plan` function). 15 tests plus a doctest; workspace at 94 tests, `angreal check all` clean.

**The totality test found a real gap in [[HLIN-S-0002]].** Asserting the governance rule that every envelope has an accepting kind other than `raw` failed on `options.v1`, and the rule was right: `options.v1` is not the same kind of thing as the others. It is what a `select` control's options endpoint returns, read by the shell as a chooser, never drawn as a panel by any kind.

Resolved by giving envelopes a **role**, panel or parameter, and amending the specification to record it:

- `hlin_manifest::envelope::Role`, `role(name)`, `PANEL_VOCABULARY` and `is_panel_envelope` name the distinction.
- Panel validation gained `PanelDefect::NotAPanelEnvelope`, so a manifest declaring `"envelope": "options.v1"` on a panel is rejected rather than rendered as an unreadable key/value tree.
- The governance rule now applies to panel envelopes only. `raw` still accepts every envelope regardless of role, so nothing reaching a renderer is undrawable.
- An envelope the shell has *never heard of* is still accepted at manifest time and fails when its data arrives, since the vocabulary grows at platform speed and a panel should not vanish from the picker because this shell is behind. Only a name known to be the wrong role is rejected.

Other decisions taken during the work:

- **`plan` rather than `render`.** The function decides what to draw and returns a description; the drawing is the design system's, when it exists. Naming it `render` would have promised something it does not do.
- **Two fallbacks make the function total**, and both are tested. A kind that cannot draw what arrived defers to `raw`, which covers a mismatched pairing slipping past validation. And a state that shows no data drops the envelope rather than handing it to a kind that would misrepresent it.
- **Only `unreachable` keeps showing the last envelope**, dimmed. The other four causes mean the data is wrong, gone, or not this viewer's to see, and showing it would mislead. During an incident, aged data with its age on screen beats an empty box.
- **`Cause::offers_successor` covers `deprecated` and `unknown`.** The specification names a successor hint for both, and it is only useful where one might exist.
- **`PanelState::all()` and `Cause::all()` exist for the totality test**, so the test walks the same enumeration the runtime uses. A state added without a treatment fails the build.

On the risk this task recorded, sketching real components: nothing here draws anything. `RenderPlan` names an effective kind, a `Treatment` and a `Notice`, and stops.

State *transitions* are documented in [[HLIN-S-0002]] but deliberately not implemented here; they belong to the aggregator, which owns the machine per [[HLIN-A-0001]]. [[HLIN-T-0005]] specifies how they travel.
