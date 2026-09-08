---
id: amend-vision-for-runtime
level: task
title: "Amend vision for runtime enforcement and vocabulary growth"
short_code: "HLIN-T-0008"
created_at: 2026-09-07T12:14:01.244710+00:00
updated_at: 2026-09-07T13:31:52.938018+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# Amend vision for runtime enforcement and vocabulary growth

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Bring the published vision ([[HLIN-V-0001]]) and the README into line with two decisions that corrected it. The vision is the owner's document, so this task proposes exact replacement text and applies it on approval.

## Acceptance Criteria

## Acceptance Criteria

- [x] Principles, "The manifest is a public API": the "fails the build that attempts it" sentence replaced with runtime detection ([[HLIN-A-0002]])
- [x] Architecture, "Views": the registry's location corrected to a companion crate, with the rebuild question answered in the same sentence ([[HLIN-A-0005]]). See the correction below: the sentence this criterion quoted does not exist
- [x] The same edits applied to `README.md`, which carries the vision text verbatim
- [x] A dated Amendments section in the vision recording both changes and the decisions that motivated them
- [x] "Open design questions" became "Design Questions, Answered", each naming the decision that closed it; the crate list drops the "if it does not live in the design system" qualifier and describes what `hlin-view` actually holds

## Implementation Notes

### Dependencies
None. Requires the owner's approval of the wording before applying.

## Status Updates

**2026-09-07 — complete.** Both documents amended, with the owner's approval of the wording taken before applying anything.

**A correction I owe the record.** This task's second criterion quoted the vision as saying the registry's placement "keeps the shell out of the path of vocabulary growth", and attributed that sentence to the document during the design review. It is not in the vision and never was: I paraphrased the section's implication and then quoted my own paraphrase back as if it were the text. The underlying point stood, because the Architecture section did say the registry "lives in the design system", which [[HLIN-A-0005]] changed. But the quotation was wrong, and a review that misquotes the thing it is reviewing is worth flagging rather than quietly fixing.

What was actually changed, in both `.metis/vision.md` and `README.md`:

- **Enforcement.** "…is a breaking change and fails the build that attempts it" became "…is a breaking change; the shell detects it at runtime and flags it, and no build anywhere has to fail for the contract to be enforced."
- **The view registry.** "The view registry lives in the design system rather than in the shell application" became a companion crate beside it, with the rebuild question answered in the same sentence: platforms never trigger a shell rebuild themselves.
- **The open questions became answers.** All five now name the specification or decision that closed them, so a reader of the vision alone can find them. The heading changed with them, since "Open design questions" listing five closed ones would be its own small lie.
- **The crate list** drops the "if it does not live in the design system" qualifier and says what `hlin-view` holds.
- **An Amendments section** at the foot of the vision records both changes, dated, with the decisions that motivated them.

One process note. Partway through, a `git checkout` meant to clean up a stuck command reverted both files and undid amendments that were not yet committed. They were reapplied and verified, and the final state is correct, but the near-miss is a good argument for committing prose edits as soon as they are made rather than batching them behind the rest of a task.
