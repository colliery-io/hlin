//! Everything draws, and the things that matter are visible in what it draws.
//!
//! A pack is where "rendering is total" stops being a property of types and
//! becomes a property of pixels, so these render to HTML and look at it.

use hlin_manifest::Envelope;
use hlin_manifest::envelope::VOCABULARY as ENVELOPES;
use hlin_pack_demo::DemoPack;
use hlin_view::kind::VOCABULARY as KINDS;
use hlin_view::pack::{Context, draw};
use hlin_view::render::Treatment;
use hlin_view::{Cause, DesignPack, Kind, PanelState, plan};
use leptos::prelude::*;

fn envelope(name: &str) -> Envelope {
    let document = match name {
        "scalar.v1" => {
            r#"{ "envelope": "scalar.v1", "value": 1523, "unit": "per_second",
                 "label": "records/s", "previous": 1490 }"#
        }
        "series.v1" => {
            r#"{ "envelope": "series.v1", "unit": "per_second", "series": [
                 { "name": "worker-a", "points": [[1757244600000, 12.5], [1757244660000, null], [1757244720000, 13.1]] },
                 { "name": "worker-b", "points": [[1757244600000, 9.0], [1757244660000, 9.4], [1757244720000, 9.9]] }] }"#
        }
        "records.v1" => {
            r#"{ "envelope": "records.v1",
                 "columns": [{ "key": "stage", "label": "Stage", "type": "string" },
                             { "key": "depth", "label": "Depth", "type": "number", "unit": "count" }],
                 "rows": [{ "stage": "parse", "depth": 41 }, { "stage": "index" }] }"#
        }
        "status.v1" => {
            r#"{ "envelope": "status.v1", "status": "degraded", "label": "Ingest",
                 "detail": "1 of 3 restarting",
                 "items": [{ "name": "worker-a", "status": "ok" },
                           { "name": "worker-b", "status": "down", "detail": "restarting" }] }"#
        }
        "options.v1" => {
            r#"{ "envelope": "options.v1", "options": [
                 { "value": "us-east", "label": "US East" },
                 { "value": "lab", "label": "Lab" }] }"#
        }
        other => panic!("no fixture for `{other}`; the envelope vocabulary grew"),
    };
    hlin_manifest::parse_envelope(document.as_bytes(), name).expect("fixture is valid")
}

/// Render a view to HTML, so a test can look at what a person would see.
fn html(view: AnyView) -> String {
    view.to_html()
}

// -- Totality, at the level of output -------------------------------------

#[test]
fn every_kind_in_every_state_draws_something() {
    let pack = DemoPack;
    let mut drawn = 0;

    for kind in KINDS {
        for state in PanelState::all() {
            // With nothing received.
            let empty = html(draw(&plan(state, kind, None), &pack, None));
            assert!(
                !empty.trim().is_empty(),
                "{kind} in {state} with no data drew nothing"
            );
            drawn += 1;

            // And with each envelope this kind accepts.
            for name in kind.accepts() {
                let document = envelope(name);
                let rendered = html(draw(&plan(state, kind, Some(&document)), &pack, Some(12)));
                assert!(
                    !rendered.trim().is_empty(),
                    "{kind} in {state} with {name} drew nothing"
                );
                drawn += 1;
            }
        }
    }

    assert!(drawn > 100, "only {drawn} combinations drawn");
}

#[test]
fn every_envelope_reaches_a_rendering_under_raw() {
    // `raw` accepts everything, which is what makes rendering total. For the
    // four panel envelopes it draws the document as a readable tree and names
    // the type. `options.v1` is the exception: the pack has a better rendering
    // for a list of choices than a JSON tree, and `draw` routes to it, so the
    // guarantee here is that something sensible appears rather than that a
    // particular shape does.
    let pack = DemoPack;

    for name in ENVELOPES {
        let document = envelope(name);
        let rendered = html(draw(
            &plan(PanelState::Ready, Kind::Raw, Some(&document)),
            &pack,
            None,
        ));
        assert!(!rendered.trim().is_empty(), "raw drew nothing for {name}");

        if name == "options.v1" {
            assert!(rendered.contains("US East"), "choices are shown as choices");
        } else {
            assert!(
                rendered.contains(name),
                "raw should name the envelope it is showing, got: {rendered}"
            );
        }
    }
}

#[test]
fn a_pack_can_draw_choices_for_a_control() {
    // The same method a select control will call directly once the frontend
    // has one; it is in the trait because a pack must be able to draw choices,
    // not only panels.
    let document = envelope("options.v1");
    let Envelope::Options(options) = &document else {
        unreachable!()
    };
    let rendered = html(DemoPack.options(
        options,
        Context {
            treatment: Treatment::Normal,
            ..Context::ready()
        },
    ));

    assert!(rendered.contains("US East"));
    assert!(rendered.contains("us-east"));
}

// -- The things a viewer needs to be true ---------------------------------

#[test]
fn a_gap_is_drawn_as_a_gap_rather_than_a_line_to_zero() {
    // The classic way a chart lies. The fixture's first series has a null in
    // the middle, which must start a new subpath rather than join across it.
    let document = envelope("series.v1");
    let rendered = html(draw(
        &plan(PanelState::Ready, Kind::Timeseries, Some(&document)),
        &DemoPack,
        None,
    ));

    let worker_a = rendered
        .split("stroke=")
        .nth(1)
        .expect("at least one line is drawn");
    let moves = worker_a.matches('M').count();
    assert!(
        moves >= 2,
        "a series with a gap should have two subpaths, got: {worker_a}"
    );
}

#[test]
fn a_stat_shows_its_value_its_label_and_its_change() {
    let document = envelope("scalar.v1");
    let rendered = html(draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&document)),
        &DemoPack,
        None,
    ));

    assert!(rendered.contains("1523/s"), "the value with its unit");
    assert!(rendered.contains("records/s"), "the label");
    assert!(rendered.contains('%'), "the change against previous");
}

#[test]
fn a_table_leaves_a_missing_cell_empty_rather_than_dropping_the_row() {
    let document = envelope("records.v1");
    let rendered = html(draw(
        &plan(PanelState::Ready, Kind::Table, Some(&document)),
        &DemoPack,
        None,
    ));

    assert!(rendered.contains("parse"));
    assert!(
        rendered.contains("index"),
        "a row missing a declared key is still a row"
    );
    assert_eq!(rendered.matches("<tr>").count(), 3, "a header and two rows");
}

#[test]
fn a_series_drawn_as_a_table_has_one_column_per_series() {
    let document = envelope("series.v1");
    let rendered = html(draw(
        &plan(PanelState::Ready, Kind::Table, Some(&document)),
        &DemoPack,
        None,
    ));

    assert!(rendered.contains("worker-a"));
    assert!(rendered.contains("worker-b"));
    assert!(rendered.contains("Time"));
}

#[test]
fn a_status_shows_the_rollup_and_its_parts() {
    let document = envelope("status.v1");
    let rendered = html(draw(
        &plan(PanelState::Ready, Kind::Status, Some(&document)),
        &DemoPack,
        None,
    ));

    assert!(rendered.contains("degraded"));
    assert!(rendered.contains("Ingest"));
    assert!(rendered.contains("worker-b"));
    assert!(rendered.contains("restarting"));
}

// -- What each state looks like -------------------------------------------

#[test]
fn a_stale_panel_shows_its_data_and_its_age() {
    let document = envelope("scalar.v1");
    let rendered = html(draw(
        &plan(PanelState::Stale, Kind::Stat, Some(&document)),
        &DemoPack,
        Some(150),
    ));

    assert!(rendered.contains("1523/s"), "stale still shows the data");
    assert!(rendered.contains("2m ago"), "and says how old it is");
    assert!(rendered.contains("aged"));
}

#[test]
fn an_unreachable_panel_keeps_its_data_and_says_why() {
    let document = envelope("scalar.v1");
    let rendered = html(draw(
        &plan(
            PanelState::Unavailable(Cause::Unreachable),
            Kind::Stat,
            Some(&document),
        ),
        &DemoPack,
        Some(600),
    ));

    assert!(
        rendered.contains("1523/s"),
        "during an incident, aged data beats an empty box"
    );
    assert!(rendered.contains("dimmed"));
    assert!(rendered.contains("unreachable"));
}

#[test]
fn the_other_causes_show_nothing_and_explain_themselves() {
    let document = envelope("scalar.v1");

    for (cause, expected) in [
        (Cause::Malformed, "unreadable"),
        (Cause::Unknown, "no longer exists"),
        (Cause::Deprecated, "retired"),
        (Cause::Forbidden, "do not have access"),
    ] {
        let rendered = html(draw(
            &plan(PanelState::Unavailable(cause), Kind::Stat, Some(&document)),
            &DemoPack,
            None,
        ));

        assert!(
            !rendered.contains("1523"),
            "{cause} must not show data that is wrong, gone, or not theirs"
        );
        assert!(
            rendered.contains(expected),
            "{cause} should say `{expected}`, got: {rendered}"
        );
    }
}

#[test]
fn a_retired_panel_mentions_that_a_replacement_may_exist() {
    let rendered = html(draw(
        &plan(PanelState::Unavailable(Cause::Deprecated), Kind::Stat, None),
        &DemoPack,
        None,
    ));
    assert!(rendered.contains("replacement"));
}

#[test]
fn a_loading_panel_is_shaped_like_what_is_coming() {
    for kind in KINDS {
        let rendered = html(draw(
            &plan(PanelState::Loading, kind, None),
            &DemoPack,
            None,
        ));
        assert!(
            rendered.contains("skeleton"),
            "{kind} should draw a skeleton while loading"
        );
    }
}

// -- The pack's own guarantees --------------------------------------------

#[test]
fn nothing_a_platform_wrote_reaches_a_viewer_in_a_placeholder() {
    // The explanation is the shell's words, chosen from a closed set of
    // causes. A platform's error body has no path to this text.
    let rendered = html(draw(
        &plan(PanelState::Unavailable(Cause::Malformed), Kind::Stat, None),
        &DemoPack,
        None,
    ));
    assert!(rendered.contains("this platform sent something unreadable"));
}

#[test]
fn every_panel_is_scoped_so_it_cannot_leak_into_a_host_page() {
    let document = envelope("scalar.v1");
    for treatment in [
        Treatment::Normal,
        Treatment::Aged,
        Treatment::Dimmed,
        Treatment::Skeleton,
        Treatment::Placeholder,
    ] {
        let context = Context {
            treatment,
            ..Context::ready()
        };
        let rendered = html(hlin_pack_demo::DemoPack.stat(
            match &document {
                Envelope::Scalar(scalar) => scalar,
                _ => unreachable!(),
            },
            context,
        ));
        assert!(rendered.contains(hlin_pack_demo::ROOT_CLASS));
    }
}

#[test]
fn the_stylesheet_only_styles_this_pack() {
    // One exception, and it is the mechanism rather than a leak: a pack fills
    // the shell's chrome properties on `:root`, because that is where the frame
    // Hlin draws around these panels reads them from. Anything else unscoped
    // would reach into a host page, which is what this test exists to prevent.
    let scope = format!(".{}", hlin_pack_demo::ROOT_CLASS);
    let mut in_comment = false;
    let mut in_root = false;

    for line in hlin_pack_demo::STYLESHEET.lines() {
        let line = line.trim();

        if in_comment {
            in_comment = !line.ends_with("*/");
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if line.starts_with("/*") {
            in_comment = !line.ends_with("*/");
            continue;
        }

        if in_root {
            if line == "}" {
                in_root = false;
                continue;
            }
            assert!(
                line.starts_with("--hlin-"),
                "the `:root` block may only fill the shell's chrome properties, \
                 and this is styling something: {line}"
            );
            continue;
        }

        if line.starts_with(":root") {
            in_root = true;
            continue;
        }

        assert!(
            line.starts_with(&scope),
            "every rule must be scoped, found: {line}"
        );
    }

    assert!(!in_root, "the `:root` block is not closed");
    assert!(
        hlin_pack_demo::STYLESHEET.contains("--hlin-surface"),
        "this pack fills the chrome properties, and a test that passed without \
         them would not be checking the exception it allows"
    );
}

#[test]
fn a_pack_can_be_held_behind_a_trait_object() {
    // Not used anywhere today, and worth knowing it is available: with the view
    // type named, `DesignPack` is object-safe, so a frontend could choose among
    // several compiled-in packs at runtime rather than at build time.
    let packs: Vec<Box<dyn hlin_view::DesignPack<View = leptos::prelude::AnyView>>> =
        vec![Box::new(hlin_pack_demo::DemoPack)];

    assert_eq!(packs.len(), 1);
    assert!(!packs[0].stylesheet().is_empty());
}
