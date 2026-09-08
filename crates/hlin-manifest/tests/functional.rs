//! Contract evolution, as tests.
//!
//! A platform ships, ships again, and one day ships something it should have
//! called a major. These tests walk the diff classification table from
//! specification HLIN-S-0001 row by row, and then the verdicts from decision
//! HLIN-A-0002: what the shell does about what it found.

use hlin_manifest::{Change, Class, Manifest, Verdict, classify_diff};

/// A manifest at `version` whose panels are exactly `panels`.
fn revision(version: &str, panels: &str) -> Manifest {
    hlin_manifest::parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "{version}",
          "platform": {{ "id": "orebank", "name": "Orebank" }},
          "panels": [{panels}],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

const THROUGHPUT: &str = r#"
    { "key": "throughput", "title": "Throughput", "kind": "timeseries",
      "envelope": "series.v1", "data": "api/hlin/throughput",
      "params": ["time_range"] }
"#;

fn assert_classes(before: &Manifest, after: &Manifest, expected: Class) {
    let report = classify_diff(before, after);
    assert!(!report.is_unchanged(), "expected some change");
    assert_eq!(
        report.class, expected,
        "unexpected class for changes {:?}",
        report.changes
    );
}

// -- Nothing moved --------------------------------------------------------

#[test]
fn an_identical_manifest_is_no_change() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision("1.0.0", THROUGHPUT);
    let report = classify_diff(&before, &after);
    assert!(report.is_unchanged());
    assert_eq!(report.verdict, Verdict::NoChange);
}

#[test]
fn bumping_the_version_alone_is_no_change() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision("2.0.0", THROUGHPUT);
    assert_eq!(classify_diff(&before, &after).verdict, Verdict::NoChange);
}

// -- Additive -------------------------------------------------------------

#[test]
fn adding_a_panel_is_additive() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "1.1.0",
        &format!(
            r#"{THROUGHPUT},
            {{ "key": "queue-depth", "title": "Queue depth", "kind": "stat",
               "envelope": "scalar.v1", "data": "api/hlin/queue-depth" }}"#
        ),
    );
    assert_classes(&before, &after, Class::Additive);
    assert_eq!(classify_diff(&before, &after).verdict, Verdict::Accepted);
}

#[test]
fn adding_a_control_is_additive() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "1.1.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "series.v1", "data": "api/hlin/throughput",
             "params": ["time_range",
                        { "param": "select", "id": "cluster", "label": "Cluster", "options": "api/c" }] }"#,
    );
    assert_classes(&before, &after, Class::Additive);
}

#[test]
fn deprecating_a_panel_is_additive() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "1.1.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "series.v1", "data": "api/hlin/throughput",
             "params": ["time_range"],
             "lifecycle": { "status": "deprecated", "sunset": "2027-01-01" } }"#,
    );
    assert_classes(&before, &after, Class::Additive);
}

#[test]
fn extending_a_deprecation_window_is_additive() {
    let deprecated = |sunset: &str| {
        revision(
            "1.0.0",
            &format!(
                r#"{{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
                      "envelope": "series.v1", "data": "api/hlin/throughput",
                      "lifecycle": {{ "status": "deprecated", "sunset": "{sunset}" }} }}"#
            ),
        )
    };
    assert_classes(
        &deprecated("2027-01-01"),
        &deprecated("2027-06-01"),
        Class::Additive,
    );
}

#[test]
fn widening_a_control_is_additive() {
    let select = |multiple: bool| {
        revision(
            "1.0.0",
            &format!(
                r#"{{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
                      "envelope": "series.v1", "data": "api/hlin/throughput",
                      "params": [{{ "param": "select", "id": "cluster", "label": "Cluster",
                                    "options": "api/c", "multiple": {multiple} }}] }}"#
            ),
        )
    };
    assert_classes(&select(false), &select(true), Class::Additive);
}

// -- Non-contract ---------------------------------------------------------

#[test]
fn presentation_changes_are_not_contract() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "1.0.0",
        r#"{ "key": "throughput", "title": "Records per second", "kind": "stat",
             "envelope": "series.v1", "data": "api/hlin/throughput",
             "params": ["time_range"], "description": "now with a description" }"#,
    );
    assert_classes(&before, &after, Class::NonContract);
    let report = classify_diff(&before, &after);
    assert!(report.changes.contains(&Change::KindChanged {
        key: "throughput".to_string()
    }));
    assert_eq!(report.verdict, Verdict::Accepted);
}

#[test]
fn moving_the_health_endpoint_is_not_contract() {
    let before = revision("1.0.0", THROUGHPUT);
    let mut after = revision("1.0.0", THROUGHPUT);
    after.health = "api/healthz".to_string();
    assert_classes(&before, &after, Class::NonContract);
}

// -- Breaking -------------------------------------------------------------

#[test]
fn removing_a_panel_is_breaking() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision("2.0.0", "");
    assert_classes(&before, &after, Class::Breaking);
    assert_eq!(classify_diff(&before, &after).verdict, Verdict::Accepted);
}

#[test]
fn removing_a_panel_inside_its_window_is_flagged_as_such() {
    let before = revision(
        "1.0.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "series.v1", "data": "api/hlin/throughput",
             "lifecycle": { "status": "deprecated", "sunset": "2027-01-01" } }"#,
    );
    let after = revision("2.0.0", "");
    let report = classify_diff(&before, &after);
    assert!(report.changes.contains(&Change::PanelRemoved {
        key: "throughput".to_string(),
        within_deprecation_window: true,
    }));
}

#[test]
fn changing_the_envelope_or_the_endpoint_is_breaking() {
    let before = revision("1.0.0", THROUGHPUT);

    let new_envelope = revision(
        "2.0.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "records.v1", "data": "api/hlin/throughput", "params": ["time_range"] }"#,
    );
    assert_classes(&before, &new_envelope, Class::Breaking);

    let new_endpoint = revision(
        "2.0.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "series.v1", "data": "api/hlin/rate", "params": ["time_range"] }"#,
    );
    assert_classes(&before, &new_endpoint, Class::Breaking);
}

#[test]
fn removing_a_control_is_breaking() {
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "2.0.0",
        r#"{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
             "envelope": "series.v1", "data": "api/hlin/throughput" }"#,
    );
    assert_classes(&before, &after, Class::Breaking);
}

#[test]
fn narrowing_a_control_is_breaking() {
    let select = |multiple: bool, options: &str| {
        revision(
            "1.0.0",
            &format!(
                r#"{{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
                      "envelope": "series.v1", "data": "api/hlin/throughput",
                      "params": [{{ "param": "select", "id": "cluster", "label": "Cluster",
                                    "options": "{options}", "multiple": {multiple} }}] }}"#
            ),
        )
    };

    // Accepting one value where several used to be allowed.
    assert_classes(
        &select(true, "api/c"),
        &select(false, "api/c"),
        Class::Breaking,
    );
    // Pointing the value domain somewhere else.
    assert_classes(
        &select(true, "api/c"),
        &select(true, "api/d"),
        Class::Breaking,
    );
}

#[test]
fn shortening_a_deprecation_window_is_breaking() {
    let deprecated = |sunset: &str| {
        revision(
            "1.0.0",
            &format!(
                r#"{{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
                      "envelope": "series.v1", "data": "api/hlin/throughput",
                      "lifecycle": {{ "status": "deprecated", "sunset": "{sunset}" }} }}"#
            ),
        )
    };
    assert_classes(
        &deprecated("2027-06-01"),
        &deprecated("2027-01-01"),
        Class::Breaking,
    );
}

// -- Verdicts (decision HLIN-A-0002) --------------------------------------

#[test]
fn breaking_without_a_major_bump_is_a_violation() {
    let before = revision("1.4.0", THROUGHPUT);
    let after = revision("1.5.0", "");
    let report = classify_diff(&before, &after);
    assert_eq!(
        report.verdict,
        Verdict::Violation {
            declared: "1.5.0".parse().unwrap(),
            expected_major: 2,
        }
    );
    assert_eq!(report.breaking().len(), 1, "the signal names specifics");
}

#[test]
fn breaking_with_a_major_bump_is_ordinary_evolution() {
    let before = revision("1.4.0", THROUGHPUT);
    let after = revision("2.0.0", "");
    assert_eq!(classify_diff(&before, &after).verdict, Verdict::Accepted);
}

#[test]
fn a_version_going_backwards_is_a_rollback_not_a_violation() {
    let before = revision("2.0.0", "");
    let after = revision("1.4.0", THROUGHPUT);
    assert_eq!(
        classify_diff(&before, &after).verdict,
        Verdict::Rollback {
            from: "2.0.0".parse().unwrap(),
            to: "1.4.0".parse().unwrap(),
        }
    );
}

#[test]
fn a_rollback_that_removes_panels_is_still_not_a_violation() {
    // The incident case: a platform reverts to an older release that never had
    // the newer panels. Panels disappear, which is breaking, and the version
    // goes backwards. Flagging this would page someone mid-incident for doing
    // the right thing.
    let before = revision(
        "2.0.0",
        &format!(
            r#"{THROUGHPUT},
            {{ "key": "queue-depth", "title": "Queue depth", "kind": "stat",
               "envelope": "scalar.v1", "data": "api/hlin/queue-depth" }}"#
        ),
    );
    let after = revision("1.9.0", THROUGHPUT);
    let report = classify_diff(&before, &after);
    assert_eq!(report.class, Class::Breaking);
    assert!(matches!(report.verdict, Verdict::Rollback { .. }));
}

#[test]
fn an_additive_change_with_no_bump_at_all_is_accepted() {
    // Semver here is descriptive, not enforced: the only claim checked is that
    // breakage carries a major bump.
    let before = revision("1.0.0", THROUGHPUT);
    let after = revision(
        "1.0.0",
        &format!(
            r#"{THROUGHPUT},
            {{ "key": "queue-depth", "title": "Queue depth", "kind": "stat",
               "envelope": "scalar.v1", "data": "api/hlin/queue-depth" }}"#
        ),
    );
    assert_eq!(classify_diff(&before, &after).verdict, Verdict::Accepted);
}

#[test]
fn diffing_compares_two_manifests_and_does_not_debounce() {
    // Debounce is a question about a sequence of manifests, so it belongs to
    // the registry, which holds the sequence. Flapping between two revisions
    // classifies each way independently, every time.
    let one = revision("1.0.0", THROUGHPUT);
    let other = revision("1.0.0", "");

    assert_eq!(classify_diff(&one, &other).class, Class::Breaking);
    assert_eq!(classify_diff(&other, &one).class, Class::Additive);
    assert_eq!(classify_diff(&one, &other).class, Class::Breaking);
}
