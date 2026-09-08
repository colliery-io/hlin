//! The envelope vocabulary, as tests.
//!
//! Every worked example in specification HLIN-S-0002 round-trips here, and
//! every limit it sets has a test that crosses it.

use hlin_manifest::envelope::{ColumnType, Envelope, Health, MAX_DOCUMENT_BYTES};
use hlin_manifest::envelope_validate::{
    MAX_COLUMNS, MAX_OPTIONS, MAX_POINTS_PER_SERIES, MAX_ROWS, MAX_SERIES, MAX_STATUS_ITEMS,
};
use hlin_manifest::{EnvelopeDefect, parse_envelope};

const SCALAR: &str = r#"{ "envelope": "scalar.v1", "value": 1523, "unit": "per_second",
                          "label": "records/s", "previous": 1490, "as_of": "2026-09-07T11:30:00Z" }"#;

const SERIES: &str = r#"{
  "envelope": "series.v1",
  "unit": "per_second",
  "series": [
    { "name": "worker-a", "points": [[1757244600000, 12.5], [1757244660000, 13.1], [1757244720000, null]] },
    { "name": "worker-b", "points": [[1757244600000, 9.0],  [1757244660000, 9.4],  [1757244720000, 9.9]] }
  ]
}"#;

const RECORDS: &str = r#"{
  "envelope": "records.v1",
  "columns": [
    { "key": "stage", "label": "Stage", "type": "string" },
    { "key": "depth", "label": "Depth", "type": "number", "unit": "count" },
    { "key": "oldest", "label": "Oldest", "type": "timestamp" }
  ],
  "rows": [
    { "stage": "parse", "depth": 41, "oldest": "2026-09-07T11:29:12Z" },
    { "stage": "index", "depth": 3,  "oldest": "2026-09-07T11:30:40Z" }
  ]
}"#;

const STATUS: &str = r#"{
  "envelope": "status.v1",
  "status": "degraded",
  "label": "Ingest",
  "detail": "1 of 4 workers restarting",
  "since": "2026-09-07T11:02:00Z",
  "items": [
    { "name": "worker-a", "status": "ok" },
    { "name": "worker-b", "status": "down", "detail": "restarting" }
  ]
}"#;

const OPTIONS: &str = r#"{
  "envelope": "options.v1",
  "options": [
    { "value": "us-east", "label": "US East", "group": "Production" },
    { "value": "eu-west", "label": "EU West", "group": "Production" },
    { "value": "lab",     "label": "Lab" }
  ]
}"#;

fn accept(document: &str, promised: &str) -> Envelope {
    parse_envelope(document.as_bytes(), promised)
        .unwrap_or_else(|defect| panic!("{promised} should be accepted: {defect}"))
}

fn reject(document: &str, promised: &str) -> EnvelopeDefect {
    parse_envelope(document.as_bytes(), promised).expect_err("should be rejected")
}

// -- The specification's examples ------------------------------------------

#[test]
fn every_worked_example_round_trips() {
    for (document, promised) in [
        (SCALAR, "scalar.v1"),
        (SERIES, "series.v1"),
        (RECORDS, "records.v1"),
        (STATUS, "status.v1"),
        (OPTIONS, "options.v1"),
    ] {
        let envelope = accept(document, promised);
        assert_eq!(envelope.name(), promised);

        let reserialized = serde_json::to_string(&envelope).expect("serialises");
        let again = accept(&reserialized, promised);
        assert_eq!(envelope, again, "{promised} survives a round trip");
    }
}

#[test]
fn the_examples_carry_the_values_they_look_like() {
    let Envelope::Scalar(scalar) = accept(SCALAR, "scalar.v1") else {
        panic!("expected a scalar")
    };
    assert_eq!(scalar.value, serde_json::json!(1523));
    assert_eq!(scalar.previous, Some(1490.0));

    let Envelope::Series(series) = accept(SERIES, "series.v1") else {
        panic!("expected a series")
    };
    assert_eq!(series.series.len(), 2);
    assert_eq!(series.series[0].points[0].at(), 1_757_244_600_000);
    assert_eq!(series.series[0].points[2].value(), None, "null is a gap");

    let Envelope::Records(records) = accept(RECORDS, "records.v1") else {
        panic!("expected records")
    };
    assert_eq!(records.columns[1].value_type, ColumnType::Number);
    assert_eq!(records.rows.len(), 2);

    let Envelope::Status(status) = accept(STATUS, "status.v1") else {
        panic!("expected a status")
    };
    assert_eq!(status.status, Health::Degraded);
    assert_eq!(status.items[1].status, Health::Down);

    let Envelope::Options(options) = accept(OPTIONS, "options.v1") else {
        panic!("expected options")
    };
    assert_eq!(options.options.len(), 3);
    assert_eq!(options.options[2].group, None);
}

#[test]
fn unknown_fields_are_preserved() {
    let document = r#"{ "envelope": "scalar.v1", "value": 1, "confidence": 0.9 }"#;
    let Envelope::Scalar(scalar) = accept(document, "scalar.v1") else {
        panic!("expected a scalar")
    };
    assert!(scalar.extra.contains_key("confidence"));

    let reserialized = serde_json::to_string(&scalar).unwrap();
    assert!(reserialized.contains("confidence"));
}

#[test]
fn as_of_is_readable_wherever_it_appears() {
    assert!(accept(SCALAR, "scalar.v1").as_of().is_some());
    assert!(accept(SERIES, "series.v1").as_of().is_none());

    let with_as_of = SERIES.replace(
        r#""unit": "per_second","#,
        r#""unit": "per_second", "as_of": "2026-09-07T11:30:00Z","#,
    );
    assert!(accept(&with_as_of, "series.v1").as_of().is_some());
}

// -- Self-declaration (REQ-1.1) -------------------------------------------

#[test]
fn a_document_must_be_the_type_the_panel_promised() {
    assert_eq!(
        reject(SCALAR, "series.v1"),
        EnvelopeDefect::Mismatch {
            promised: "series.v1".to_string(),
            returned: "scalar.v1".to_string(),
        }
    );
}

#[test]
fn a_document_must_declare_what_it_is() {
    let anonymous = r#"{ "value": 1 }"#;
    assert!(matches!(
        reject(anonymous, "scalar.v1"),
        EnvelopeDefect::Unreadable { .. }
    ));
}

#[test]
fn an_envelope_outside_the_vocabulary_is_rejected_from_either_side() {
    let future = r#"{ "envelope": "distribution.v1", "buckets": [] }"#;
    assert_eq!(
        reject(future, "distribution.v1"),
        EnvelopeDefect::UnknownEnvelope {
            declared: "distribution.v1".to_string()
        }
    );
    assert_eq!(
        reject(future, "scalar.v1"),
        EnvelopeDefect::UnknownEnvelope {
            declared: "distribution.v1".to_string()
        }
    );
}

// -- Limits, none of which truncate (REQ-3.1) -----------------------------

#[test]
fn a_document_over_the_size_limit_is_rejected_rather_than_truncated() {
    let padding = "x".repeat(MAX_DOCUMENT_BYTES);
    let oversized = format!(r#"{{ "envelope": "scalar.v1", "value": 1, "note": "{padding}" }}"#);
    assert!(matches!(
        reject(&oversized, "scalar.v1"),
        EnvelopeDefect::TooLarge { .. }
    ));
}

#[test]
fn a_series_document_must_carry_at_least_one_series() {
    let empty = r#"{ "envelope": "series.v1", "series": [] }"#;
    assert_eq!(
        reject(empty, "series.v1"),
        EnvelopeDefect::Empty {
            collection: "series"
        }
    );
}

#[test]
fn too_many_series_is_rejected() {
    let lines: Vec<String> = (0..=MAX_SERIES)
        .map(|index| format!(r#"{{ "name": "s{index}", "points": [] }}"#))
        .collect();
    let document = format!(
        r#"{{ "envelope": "series.v1", "series": [{}] }}"#,
        lines.join(",")
    );
    assert_eq!(
        reject(&document, "series.v1"),
        EnvelopeDefect::OverLimit {
            collection: "series",
            count: MAX_SERIES + 1,
            limit: MAX_SERIES,
        }
    );
}

#[test]
fn too_many_points_is_rejected() {
    let points: Vec<String> = (0..=MAX_POINTS_PER_SERIES)
        .map(|index| format!("[{index},1]"))
        .collect();
    let document = format!(
        r#"{{ "envelope": "series.v1", "series": [{{ "name": "s", "points": [{}] }}] }}"#,
        points.join(",")
    );
    assert!(matches!(
        reject(&document, "series.v1"),
        EnvelopeDefect::OverLimit {
            collection: "points",
            ..
        }
    ));
}

#[test]
fn points_out_of_order_are_rejected() {
    let document = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [[2000, 1.0], [1000, 2.0]] }] }"#;
    assert_eq!(
        reject(document, "series.v1"),
        EnvelopeDefect::UnorderedPoints {
            series: "s".to_string()
        }
    );
}

#[test]
fn two_series_may_not_share_a_name() {
    let document = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [] }, { "name": "s", "points": [] }] }"#;
    assert!(matches!(
        reject(document, "series.v1"),
        EnvelopeDefect::Duplicate { .. }
    ));
}

#[test]
fn a_table_must_declare_columns_and_stay_within_its_limits() {
    let no_columns = r#"{ "envelope": "records.v1", "columns": [], "rows": [] }"#;
    assert_eq!(
        reject(no_columns, "records.v1"),
        EnvelopeDefect::Empty {
            collection: "columns"
        }
    );

    let columns: Vec<String> = (0..=MAX_COLUMNS)
        .map(|index| format!(r#"{{ "key": "c{index}", "label": "C", "type": "string" }}"#))
        .collect();
    let too_wide = format!(
        r#"{{ "envelope": "records.v1", "columns": [{}], "rows": [] }}"#,
        columns.join(",")
    );
    assert!(matches!(
        reject(&too_wide, "records.v1"),
        EnvelopeDefect::OverLimit {
            collection: "columns",
            ..
        }
    ));

    let rows: Vec<String> = (0..=MAX_ROWS)
        .map(|_| r#"{ "c": "x" }"#.to_string())
        .collect();
    let too_long = format!(
        r#"{{ "envelope": "records.v1",
              "columns": [{{ "key": "c", "label": "C", "type": "string" }}],
              "rows": [{}] }}"#,
        rows.join(",")
    );
    assert!(matches!(
        reject(&too_long, "records.v1"),
        EnvelopeDefect::OverLimit {
            collection: "rows",
            ..
        }
    ));
}

#[test]
fn rows_are_forgiving_about_keys() {
    // A missing key renders empty and an undeclared key is ignored: a platform
    // adding a field to its rows must not break the panel.
    let document = r#"{ "envelope": "records.v1",
        "columns": [{ "key": "stage", "label": "Stage", "type": "string" },
                    { "key": "depth", "label": "Depth", "type": "number" }],
        "rows": [{ "stage": "parse" }, { "stage": "index", "depth": 3, "extra": true }] }"#;
    let Envelope::Records(records) = accept(document, "records.v1") else {
        panic!("expected records")
    };
    assert!(!records.rows[0].contains_key("depth"));
    assert!(records.rows[1].contains_key("extra"));
}

#[test]
fn too_many_status_items_or_options_is_rejected() {
    let items: Vec<String> = (0..=MAX_STATUS_ITEMS)
        .map(|index| format!(r#"{{ "name": "i{index}", "status": "ok" }}"#))
        .collect();
    let document = format!(
        r#"{{ "envelope": "status.v1", "status": "ok", "items": [{}] }}"#,
        items.join(",")
    );
    assert!(matches!(
        reject(&document, "status.v1"),
        EnvelopeDefect::OverLimit {
            collection: "status items",
            ..
        }
    ));

    let choices: Vec<String> = (0..=MAX_OPTIONS)
        .map(|index| format!(r#"{{ "value": "v{index}", "label": "L" }}"#))
        .collect();
    let document = format!(
        r#"{{ "envelope": "options.v1", "options": [{}] }}"#,
        choices.join(",")
    );
    assert!(matches!(
        reject(&document, "options.v1"),
        EnvelopeDefect::OverLimit {
            collection: "options",
            ..
        }
    ));
}

#[test]
fn no_options_is_an_answer_rather_than_a_failure() {
    let document = r#"{ "envelope": "options.v1", "options": [] }"#;
    let Envelope::Options(options) = accept(document, "options.v1") else {
        panic!("expected options")
    };
    assert!(options.options.is_empty());
}

#[test]
fn two_options_may_not_share_a_value() {
    let document = r#"{ "envelope": "options.v1",
        "options": [{ "value": "a", "label": "A" }, { "value": "a", "label": "Also A" }] }"#;
    assert!(matches!(
        reject(document, "options.v1"),
        EnvelopeDefect::Duplicate { .. }
    ));
}

// -- Envelopes carry no presentation (REQ-1.2) ----------------------------

#[test]
fn presentation_has_nowhere_to_go_in_an_envelope() {
    // A platform that tries to say how something should look finds the field
    // captured as an unknown, where nothing reads it. The shell renders
    // everything; thresholds and colours are the viewer's to set.
    let document = r#"{ "envelope": "scalar.v1", "value": 1,
                        "color": "red", "threshold": 90, "width": 4 }"#;
    let Envelope::Scalar(scalar) = accept(document, "scalar.v1") else {
        panic!("expected a scalar")
    };
    for ignored in ["color", "threshold", "width"] {
        assert!(scalar.extra.contains_key(ignored));
    }
}

#[test]
fn a_point_timestamp_is_read_exactly_or_not_at_all() {
    // Epoch milliseconds are integers. A platform that sends a float where a
    // timestamp belongs is refused rather than rounded: silently truncating
    // would move a sample in time, and nothing downstream would know.
    let fractional = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [[1757244600000.5, 1.0]] }] }"#;
    assert!(matches!(
        reject(fractional, "series.v1"),
        EnvelopeDefect::Unreadable { .. }
    ));

    // A whole number written as a float is still a whole number, and serde
    // reads it as one.
    let whole = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [[1757244600000.0, 1.0]] }] }"#;
    match parse_envelope(whole.as_bytes(), "series.v1") {
        Ok(Envelope::Series(series)) => {
            assert_eq!(series.series[0].points[0].at(), 1_757_244_600_000);
        }
        Ok(_) => panic!("expected a series"),
        Err(EnvelopeDefect::Unreadable { .. }) => {
            // Also acceptable: refusing 1e12-as-float loses nothing, since a
            // platform can always write the integer.
        }
        Err(other) => panic!("unexpected defect: {other}"),
    }

    // Values, unlike timestamps, are genuinely fractional.
    let value = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [[1757244600000, 12.5]] }] }"#;
    let Envelope::Series(series) = accept(value, "series.v1") else {
        panic!("expected a series")
    };
    assert_eq!(series.series[0].points[0].value(), Some(12.5));

    // An integer value is read as a number, not refused for lacking a point.
    let integral = r#"{ "envelope": "series.v1",
        "series": [{ "name": "s", "points": [[1757244600000, 12]] }] }"#;
    let Envelope::Series(series) = accept(integral, "series.v1") else {
        panic!("expected a series")
    };
    assert_eq!(series.series[0].points[0].value(), Some(12.0));
}
