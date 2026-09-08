//! One document of each envelope type, for tests that need something to draw.

use hlin_manifest::Envelope;

/// A minimal, valid document of the named envelope type.
///
/// Panics on an unknown name, which is the right outcome: it means the envelope
/// vocabulary grew and this fixture was not updated, so the totality tests
/// would otherwise quietly stop covering the new type.
pub fn envelope_named(name: &str) -> Envelope {
    let document = match name {
        "scalar.v1" => r#"{ "envelope": "scalar.v1", "value": 1523, "unit": "per_second" }"#,
        "series.v1" => {
            r#"{ "envelope": "series.v1",
                 "series": [{ "name": "worker-a", "points": [[1757244600000, 12.5]] }] }"#
        }
        "records.v1" => {
            r#"{ "envelope": "records.v1",
                 "columns": [{ "key": "stage", "label": "Stage", "type": "string" }],
                 "rows": [{ "stage": "parse" }] }"#
        }
        "status.v1" => r#"{ "envelope": "status.v1", "status": "ok", "label": "Ingest" }"#,
        "options.v1" => {
            r#"{ "envelope": "options.v1",
                 "options": [{ "value": "us-east", "label": "US East" }] }"#
        }
        other => panic!("no fixture for envelope `{other}`; the vocabulary grew"),
    };

    hlin_manifest::parse_envelope(document.as_bytes(), name)
        .unwrap_or_else(|defect| panic!("fixture for {name} should be valid: {defect}"))
}
