---
id: hlin-manifest-envelope-types
level: task
title: "hlin-manifest: envelope types, limits and validation"
short_code: "HLIN-T-0003"
created_at: 2026-09-07T12:13:54.794663+00:00
updated_at: 2026-09-07T13:05:22.882909+00:00
parent: HLIN-I-0001
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0001
---

# hlin-manifest: envelope types, limits and validation

## Parent Initiative

[[HLIN-I-0001]]

## Objective

Encode the five v1 envelopes from [[HLIN-S-0002]] in `hlin-manifest`: `scalar.v1`, `series.v1`, `records.v1`, `status.v1`, `options.v1`, with self-declaration, limits, and validation. The aggregator will use these to turn a fetched document into either a typed envelope or a malformed reason.

## Acceptance Criteria

## Acceptance Criteria

- [x] An `Envelope` enum internally tagged on the `envelope` field, one variant per v1 envelope, with unknown fields preserved
- [x] Each example in the spec round-trips to an equal value
- [x] `parse_envelope(bytes, promised: &str)` rejects a document whose self-declaration differs from the panel's declared envelope (REQ-1.1) with a structured reason
- [x] Limits enforced with a reason naming the limit: 20 series / 5 000 points, 50 columns / 1 000 rows, 200 status items, 500 options, 1 MiB document (REQ-3.1); no truncation path exists
- [x] `series.v1` points must be ascending by time; `records.v1` rows with keys not in `columns` are ignored, not rejected
- [x] `unit` accepts the named set and any other string
- [x] `as_of` parsed as RFC 3339 and exposed for the aggregator's staleness use, via `Envelope::as_of()` on every variant

## Implementation Notes

### Technical Approach
Same serde conventions as [[HLIN-T-0002]]. Validation is a separate pass after deserialization so that reasons are specific (which limit, which series). Size limit is checked on the byte length before parsing.

### Dependencies
[[HLIN-T-0001]]. Independent of [[HLIN-T-0002]] beyond sharing crate conventions.

### Risk Considerations
Epoch-millisecond points as `[i64, Option<f64>]` tuples; ensure serde accepts both integer and float JSON numbers for the timestamp position without silently truncating.

## Status Updates

**2026-09-07 — complete.** Two modules added to `hlin-manifest`: `envelope` (the five v1 types) and `envelope_validate` (self-declaration check and limits). 20 tests in a new `tests/envelopes.rs` target; workspace now at 77 tests, `angreal check all` clean.

Decisions taken during the work:

- **Self-declaration is read before the document is parsed as an envelope.** A document of the wrong type is reported as `Mismatch { promised, returned }` rather than as a deserialisation error, because "you promised a series and sent a table" is a far more useful thing to put in front of an operator than a message about an unknown enum variant.
- **The size limit is checked on bytes, before parsing.** A document over the limit is never deserialised, so an oversized payload costs the shell nothing but the read.
- **Validation runs as a pass after deserialisation**, so defects name the specific limit and the specific series or column. There is no truncation path anywhere in the module, which the tests assert by crossing every limit and expecting rejection.
- **Uniqueness is enforced where the specification says a name is unique**: series names, column keys, status item names and option values. The specification states the property without saying what happens when it is violated; rejecting the panel is consistent with everything else, and the alternative is a chart with two legend entries that cannot be told apart.
- **Rows stay forgiving.** A row missing a declared key renders empty and a row carrying an undeclared key is ignored, so a platform adding a field to its rows never breaks a panel.

On the risk this task recorded, float timestamps: `Point(i64, Option<f64>)` refuses a fractional timestamp rather than rounding it, since truncating would move a sample in time with nothing downstream aware. That is pinned in `a_point_timestamp_is_read_exactly_or_not_at_all`, which also accepts either behaviour for a whole number written as a float, because refusing that loses nothing.

Note for [[HLIN-T-0004]]: `envelope::is_known` and `envelope::VOCABULARY` are the list `hlin-view` should build its acceptance matrix against, and `EnvelopeDefect` is the reason type the aggregator maps onto `unavailable (malformed)`.
