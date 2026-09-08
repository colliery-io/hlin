//! The view registry's public surface.
//!
//! Where the functional suite proves the totality guarantee by exhaustion,
//! these check the acceptance matrix against specification HLIN-S-0002 entry by
//! entry, so a table edited in one place and not the other is caught.

use hlin_view::{Kind, accepts};

#[test]
fn the_acceptance_matrix_matches_the_specification() {
    let expected: [(Kind, &[&str]); 6] = [
        (Kind::Stat, &["scalar.v1"]),
        (Kind::Timeseries, &["series.v1"]),
        (Kind::Sparkline, &["series.v1"]),
        (Kind::Table, &["records.v1", "series.v1"]),
        (Kind::Status, &["status.v1"]),
        (
            Kind::Raw,
            &[
                "scalar.v1",
                "series.v1",
                "records.v1",
                "status.v1",
                "options.v1",
            ],
        ),
    ];

    for (kind, envelopes) in expected {
        let mut declared: Vec<&str> = kind.accepts().to_vec();
        let mut wanted: Vec<&str> = envelopes.to_vec();
        declared.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(declared, wanted, "{kind} accepts the wrong envelopes");
    }
}

#[test]
fn a_kind_refuses_an_envelope_it_cannot_draw() {
    assert!(accepts("stat", "scalar.v1"));
    assert!(!accepts("stat", "series.v1"));
    assert!(!accepts("timeseries", "records.v1"));
    assert!(!accepts("status", "scalar.v1"));
}

#[test]
fn a_table_can_draw_a_series_as_well_as_records() {
    // One row per timestamp, one column per series. It gives every chart a
    // readable form for nothing, and it is why the matrix is a table rather
    // than a pairing.
    assert!(accepts("table", "records.v1"));
    assert!(accepts("table", "series.v1"));
}

#[test]
fn switching_a_panels_rendering_needs_a_deploy_from_nobody() {
    // A series can be a chart, a sparkline or a table, so a person can change
    // how they read a panel without anyone shipping anything.
    let choices = Kind::accepting("series.v1");
    assert!(choices.contains(&Kind::Timeseries));
    assert!(choices.contains(&Kind::Sparkline));
    assert!(choices.contains(&Kind::Table));
    assert!(choices.contains(&Kind::Raw));

    // A scalar has fewer places to go, and that is honest rather than a gap.
    assert_eq!(Kind::accepting("scalar.v1"), vec![Kind::Stat, Kind::Raw]);
}

#[test]
fn an_unknown_kind_accepts_whatever_raw_accepts() {
    assert!(accepts("sankey-diagram", "series.v1"));
    assert!(accepts("sankey-diagram", "options.v1"));
}

#[test]
fn kind_names_round_trip() {
    for kind in hlin_view::kind::VOCABULARY {
        assert_eq!(Kind::resolve(kind.name()), kind);
        assert_eq!(kind.to_string(), kind.name());
    }
}
