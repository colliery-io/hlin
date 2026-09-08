//! Composition, without a browser.
//!
//! Every gesture a person makes on a surface ends in one of these methods, so
//! this is where the composition behaviour is pinned: what adding does, what
//! removing leaves behind, and which changes are worth a write to the shell.

use hlin_stream::layout::{LayoutDocument, PanelInstanceDocument, Placement};
use hlin_ui::LayoutDraft;

fn stored(panels: Vec<PanelInstanceDocument>) -> LayoutDocument {
    LayoutDocument {
        id: Some("11111111-2222-3333-4444-555555555555".to_string()),
        title: "Mine".to_string(),
        visibility: "personal".to_string(),
        owner: "ada".to_string(),
        editable: true,
        panels,
    }
}

fn identified(id: &str, x: u32, y: u32, w: u32, h: u32) -> PanelInstanceDocument {
    PanelInstanceDocument {
        id: Some(id.to_string()),
        ..PanelInstanceDocument::new("orebank", "throughput", Placement { x, y, w, h })
    }
}

#[test]
fn a_layout_as_it_arrives_is_not_yet_a_change() {
    let draft = LayoutDraft::of(stored(vec![identified("a", 0, 0, 4, 4)]));
    assert!(
        !draft.dirty(),
        "opening a surface must not write it straight back, or every viewer \
         touches every layout they look at"
    );
}

#[test]
fn an_arrangement_that_overlaps_itself_is_repaired_on_arrival() {
    // A layout written by hand, or by an older shell with a different grid.
    let draft = LayoutDraft::of(stored(vec![
        identified("a", 0, 0, 6, 4),
        identified("b", 0, 0, 6, 4),
    ]));

    let a = draft.placement_of("a").expect("the panel is there");
    let b = draft.placement_of("b").expect("the panel is there");
    assert!(
        !a.overlaps(&b),
        "{a:?} and {b:?} still cover the same cells"
    );
    assert!(
        !draft.dirty(),
        "and repairing it is not itself a change worth writing"
    );
}

#[test]
fn adding_a_panel_puts_it_at_the_first_free_position() {
    let mut draft = LayoutDraft::of(stored(vec![identified("a", 0, 0, 4, 4)]));

    draft.add("smelter", "queue-depth");

    let panels = draft.panels();
    assert_eq!(panels.len(), 2);
    assert_eq!(
        panels[1].position,
        Placement {
            x: 4,
            y: 0,
            w: 4,
            h: 4
        },
        "beside what is already there, not below it"
    );
    assert!(
        panels[1].id.is_none(),
        "the browser does not invent an identity the shell has not assigned"
    );
    assert!(draft.dirty());
}

#[test]
fn removing_a_panel_closes_the_gap_it_leaves() {
    let mut draft = LayoutDraft::of(stored(vec![
        identified("top", 0, 0, 12, 3),
        identified("bottom", 0, 3, 12, 3),
    ]));

    draft.remove("top");

    assert_eq!(draft.panels().len(), 1);
    assert_eq!(
        draft.placement_of("bottom").expect("it is still there").y,
        0,
        "what is left rises, rather than leaving a band of nothing at the top"
    );
}

#[test]
fn removing_something_that_is_not_there_changes_nothing() {
    let mut draft = LayoutDraft::of(stored(vec![identified("a", 0, 0, 4, 4)]));
    draft.remove("not-a-panel");

    assert_eq!(draft.panels().len(), 1);
    assert!(
        !draft.dirty(),
        "a gesture that did nothing must not provoke a write"
    );
}

#[test]
fn dragging_a_panel_onto_another_displaces_it() {
    let mut draft = LayoutDraft::of(stored(vec![
        identified("first", 0, 0, 6, 4),
        identified("second", 6, 0, 6, 4),
    ]));

    draft.move_to("second", 0, 0);

    assert_eq!(
        draft.placement_of("second").expect("it is there").y,
        0,
        "the dragged panel lands where it was dropped"
    );
    assert_eq!(
        draft.placement_of("first").expect("it is there").y,
        4,
        "and the one it landed on moves down"
    );
    assert!(draft.dirty());
}

#[test]
fn resizing_keeps_the_panel_on_the_grid() {
    let mut draft = LayoutDraft::of(stored(vec![identified("a", 8, 0, 4, 4)]));

    draft.resize("a", 20, 2);

    let placement = draft.placement_of("a").expect("it is there");
    assert_eq!(placement.x, 8, "its left edge does not move");
    assert_eq!(placement.w, 4, "and it grows only to the right-hand edge");
    assert_eq!(placement.h, 2);
}

#[test]
fn the_depth_of_a_surface_is_how_far_down_it_goes() {
    let draft = LayoutDraft::of(stored(vec![
        identified("a", 0, 0, 6, 3),
        identified("b", 6, 0, 6, 5),
    ]));
    assert_eq!(draft.depth(), 5);
    assert_eq!(LayoutDraft::empty().depth(), 0);
}

#[test]
fn emptying_a_title_gives_the_panel_its_platforms_name_back() {
    let mut draft = LayoutDraft::of(stored(vec![identified("a", 0, 0, 4, 4)]));

    draft.rename("a", "  Overnight throughput  ");
    assert_eq!(
        draft
            .panel("a")
            .expect("it is there")
            .title_override
            .as_deref(),
        Some("Overnight throughput"),
        "and the surrounding space is not part of what they typed"
    );

    draft.rename("a", "   ");
    assert_eq!(
        draft.panel("a").expect("it is there").title_override,
        None,
        "clearing the box means 'whatever the platform calls it', not a blank heading"
    );
}

#[test]
fn switching_a_kind_and_switching_back_are_both_expressible() {
    let mut draft = LayoutDraft::of(stored(vec![identified("a", 0, 0, 4, 4)]));

    draft.set_kind("a", Some("sparkline"));
    assert_eq!(
        draft
            .panel("a")
            .expect("it is there")
            .kind_override
            .as_deref(),
        Some("sparkline")
    );

    draft.set_kind("a", None);
    assert_eq!(
        draft.panel("a").expect("it is there").kind_override,
        None,
        "so a platform changing its default still reaches this viewer"
    );
}

#[test]
fn a_selection_belongs_to_one_panel_and_travels_by_instance() {
    let mut draft = LayoutDraft::of(stored(vec![
        identified("a", 0, 0, 4, 4),
        identified("b", 4, 0, 4, 4),
    ]));

    draft.set_selection("a", "cluster", vec!["north".to_string()]);

    let selections = draft.selections();
    assert_eq!(selections.len(), 1, "one panel chose, not both");
    assert_eq!(
        selections["a"]["cluster"],
        vec!["north".to_string()],
        "and it is keyed by the instance the stream names"
    );

    draft.set_selection("a", "cluster", vec![]);
    assert!(
        draft.selections().is_empty(),
        "choosing nothing removes the choice rather than sending an empty one"
    );
}

#[test]
fn a_panel_with_no_identity_yet_sends_no_selections() {
    let mut draft = LayoutDraft::of(stored(vec![]));
    draft.add("orebank", "throughput");

    assert!(
        draft.selections().is_empty(),
        "there is nothing for the shell to attach them to until it has written \
         the layout"
    );
}

#[test]
fn what_the_shell_wrote_replaces_what_the_browser_had() {
    let mut draft = LayoutDraft::of(stored(vec![]));
    draft.add("orebank", "throughput");
    assert!(draft.dirty());

    // The shell answers with the same panel, now carrying an identity.
    draft.written(stored(vec![identified("assigned", 0, 0, 4, 4)]));

    assert!(!draft.dirty(), "there is nothing left to write");
    assert_eq!(
        draft.panels()[0].id.as_deref(),
        Some("assigned"),
        "and the browser now names the panel the way the stream will"
    );
}

#[test]
fn a_layout_someone_else_owns_says_so() {
    let mut document = stored(vec![]);
    document.editable = false;
    let draft = LayoutDraft::of(document);

    assert!(
        !draft.editable(),
        "the surface asks this before offering an edit that would be refused"
    );
}

#[test]
fn retitling_the_layout_to_the_same_thing_is_not_a_change() {
    let mut draft = LayoutDraft::of(stored(vec![]));

    draft.retitle("Mine");
    assert!(!draft.dirty(), "typing the same title is not an edit");

    draft.retitle("  ");
    assert!(!draft.dirty(), "and a layout is not allowed to be nameless");

    draft.retitle("Overnight");
    assert!(draft.dirty());
    assert_eq!(draft.title(), "Overnight");
}
