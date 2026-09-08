//! The totality guarantee, as a test.
//!
//! Specification HLIN-S-0002 requires that every kind draws every panel state
//! (REQ-1.5) and that no envelope is left undrawable (REQ-1.4). Those are
//! claims about a cross product, so this suite walks the whole thing rather
//! than sampling it.
//!
//! The point of testing it exhaustively is that the guarantee has to survive
//! people. A kind added without a rendering, a state added without a treatment,
//! or an envelope admitted with no kind willing to draw it all fail here rather
//! than reaching a layout.

use hlin_manifest::envelope::{PANEL_VOCABULARY, VOCABULARY as ENVELOPES};
use hlin_view::kind::VOCABULARY as KINDS;
use hlin_view::{Cause, Kind, PanelState, Treatment, plan};

mod fixtures;
use fixtures::envelope_named;

#[test]
fn every_kind_draws_every_state_for_every_envelope_it_accepts() {
    let mut combinations = 0;

    for kind in KINDS {
        for state in PanelState::all() {
            // With nothing yet received.
            let empty = plan(state, kind, None);
            assert!(
                !empty.draws_data(),
                "{kind} in {state} cannot draw data it does not have"
            );
            combinations += 1;

            // And with each envelope this kind accepts.
            for name in kind.accepts() {
                let envelope = envelope_named(name);
                let drawn = plan(state, kind, Some(&envelope));
                assert_eq!(
                    drawn.kind, kind,
                    "{kind} accepts {name}, so it should not fall back"
                );
                assert_eq!(
                    drawn.draws_data(),
                    state.shows_data(),
                    "{kind} in {state} with {name}: data shown iff the state shows data"
                );
                combinations += 1;
            }
        }
    }

    // A guard against the loop silently covering nothing.
    assert!(
        combinations > 100,
        "only {combinations} combinations walked"
    );
}

#[test]
fn every_panel_envelope_is_drawable_by_a_kind_besides_raw() {
    // The governance bar in HLIN-S-0002 asks for an accepting kind other than
    // `raw` before an envelope is admitted. It applies to the envelopes a panel
    // can declare; `options.v1` is read by a control rather than drawn by a
    // kind, so it is out of scope for the rule.
    for envelope in PANEL_VOCABULARY {
        let kinds = Kind::accepting(envelope);
        assert!(
            kinds.contains(&Kind::Raw),
            "{envelope} must be drawable by raw"
        );
        assert!(
            kinds.iter().any(|kind| *kind != Kind::Raw),
            "{envelope} has no kind but raw; the governance bar requires one"
        );
    }

    // Everything, panel envelope or not, is still drawable by raw, so nothing
    // reaching a renderer is undrawable.
    for envelope in ENVELOPES {
        assert!(Kind::Raw.accepts_envelope(envelope));
    }
}

#[test]
fn raw_accepts_the_whole_envelope_vocabulary() {
    for envelope in ENVELOPES {
        assert!(
            Kind::Raw.accepts_envelope(envelope),
            "raw must accept {envelope}, or totality has a hole"
        );
    }
    assert_eq!(
        Kind::Raw.accepts().len(),
        ENVELOPES.len(),
        "raw's acceptance list has drifted from the envelope vocabulary"
    );
}

#[test]
fn a_mismatched_pairing_still_draws() {
    // Validation should reject this before it reaches a renderer. If one ever
    // slips through, the panel is readable rather than blank.
    let table = envelope_named("records.v1");
    let drawn = plan(PanelState::Ready, Kind::Stat, Some(&table));
    assert_eq!(
        drawn.kind,
        Kind::Raw,
        "stat cannot draw a table, so raw does"
    );
    assert!(drawn.draws_data());
}

#[test]
fn an_unknown_kind_name_lands_on_raw_rather_than_breaking_a_layout() {
    for name in ["sankey-diagram", "", "STAT", "timeseries-v2"] {
        assert_eq!(Kind::resolve(name), Kind::Raw, "`{name}` should fall back");
        assert!(!Kind::is_known(name));
    }
    for kind in KINDS {
        assert_eq!(Kind::resolve(kind.name()), kind);
        assert!(Kind::is_known(kind.name()));
    }
}

#[test]
fn only_unreachable_keeps_showing_what_it_last_had() {
    let scalar = envelope_named("scalar.v1");

    for cause in Cause::all() {
        let drawn = plan(PanelState::Unavailable(cause), Kind::Stat, Some(&scalar));
        let notice = drawn.notice.expect("an unavailable panel says why");
        assert_eq!(notice.cause, cause);

        match cause {
            Cause::Unreachable => {
                assert_eq!(drawn.treatment, Treatment::Dimmed);
                assert!(
                    drawn.draws_data(),
                    "during an incident, aged data beats an empty box"
                );
            }
            _ => {
                assert_eq!(drawn.treatment, Treatment::Placeholder);
                assert!(
                    !drawn.draws_data(),
                    "{cause} means the data is wrong, gone, or not this viewer's to see"
                );
            }
        }
    }
}

#[test]
fn a_panel_on_its_way_out_offers_the_successor() {
    for cause in Cause::all() {
        let drawn = plan(PanelState::Unavailable(cause), Kind::Stat, None);
        let notice = drawn.notice.expect("an unavailable panel says why");
        assert_eq!(
            notice.offer_successor,
            matches!(cause, Cause::Deprecated | Cause::Unknown),
            "{cause}: a successor is only useful where one might exist"
        );
    }
}

#[test]
fn the_working_states_carry_no_notice() {
    let scalar = envelope_named("scalar.v1");
    for state in [PanelState::Loading, PanelState::Ready, PanelState::Stale] {
        assert!(plan(state, Kind::Stat, Some(&scalar)).notice.is_none());
    }
}

#[test]
fn a_stale_panel_still_shows_its_data() {
    let scalar = envelope_named("scalar.v1");
    let drawn = plan(PanelState::Stale, Kind::Stat, Some(&scalar));
    assert_eq!(drawn.treatment, Treatment::Aged);
    assert!(drawn.draws_data(), "stale data is shown, with its age");
}

// -- The pack seam is total too -------------------------------------------

/// A pack that draws nothing and remembers what it was asked for.
///
/// Lets the totality of `draw` be checked without any UI framework, which is
/// the point of the trait being generic over its view type.
#[derive(Default)]
struct Recorder {
    calls: std::cell::RefCell<Vec<&'static str>>,
}

impl hlin_view::DesignPack for Recorder {
    type View = &'static str;

    fn stat(&self, _: &hlin_manifest::envelope::Scalar, _: hlin_view::pack::Context) -> Self::View {
        self.note("stat")
    }
    fn timeseries(
        &self,
        _: &hlin_manifest::envelope::Series,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("timeseries")
    }
    fn sparkline(
        &self,
        _: &hlin_manifest::envelope::Series,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("sparkline")
    }
    fn table(
        &self,
        _: &hlin_manifest::envelope::Records,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("table")
    }
    fn series_as_table(
        &self,
        _: &hlin_manifest::envelope::Series,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("series_as_table")
    }
    fn status(
        &self,
        _: &hlin_manifest::envelope::Status,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("status")
    }
    fn options(
        &self,
        _: &hlin_manifest::envelope::Options,
        _: hlin_view::pack::Context,
    ) -> Self::View {
        self.note("options")
    }
    /// Offers exactly one component, so both branches are reachable: a pack
    /// that drew everything would never exercise the fallback, and one that
    /// drew nothing would never exercise the forwarding.
    fn custom(
        &self,
        component: &str,
        _: &hlin_manifest::Envelope,
        _: hlin_view::pack::Context,
    ) -> Option<Self::View> {
        (component == "recorder.known").then(|| self.note("custom"))
    }
    fn offers(&self) -> &'static [&'static str] {
        &["recorder.known"]
    }
    fn raw(&self, _: &hlin_manifest::Envelope, _: hlin_view::pack::Context) -> Self::View {
        self.note("raw")
    }
    fn skeleton(&self, _: Kind, _: hlin_view::pack::Context) -> Self::View {
        self.note("skeleton")
    }
    fn placeholder(&self, _: Kind, _: hlin_view::pack::Context) -> Self::View {
        self.note("placeholder")
    }
    /// A pack with no styling of its own, which is the whole point of this one:
    /// it records calls and draws nothing.
    fn stylesheet(&self) -> &'static str {
        ""
    }
}

impl Recorder {
    fn note(&self, what: &'static str) -> &'static str {
        self.calls.borrow_mut().push(what);
        what
    }
}

#[test]
fn drawing_reaches_a_pack_method_for_every_state_and_envelope() {
    // The other half of totality. `plan` decides and never has a hole; `draw`
    // dispatches and never has one either, so between them no panel a platform
    // can declare is undrawable.
    let pack = Recorder::default();
    let mut drawn = 0;

    for kind in KINDS {
        for state in PanelState::all() {
            hlin_view::draw(&plan(state, kind, None), &pack, None);
            drawn += 1;

            for name in kind.accepts() {
                let envelope = envelope_named(name);
                hlin_view::draw(&plan(state, kind, Some(&envelope)), &pack, Some(30));
                drawn += 1;
            }
        }
    }

    assert_eq!(
        pack.calls.borrow().len(),
        drawn,
        "every draw must reach exactly one pack method"
    );
    assert!(drawn > 100, "only {drawn} combinations drawn");
}

#[test]
fn a_state_with_no_data_never_reaches_a_drawing_method() {
    // A pack's data methods must not be handed a document the state says
    // should not be shown, or a placeholder would quietly render stale values.
    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    for cause in Cause::all() {
        if cause == Cause::Unreachable {
            continue; // keeps its data, deliberately
        }
        hlin_view::draw(
            &plan(PanelState::Unavailable(cause), Kind::Stat, Some(&scalar)),
            &pack,
            None,
        );
    }

    assert!(
        pack.calls
            .borrow()
            .iter()
            .all(|call| *call == "placeholder"),
        "expected only placeholders, got {:?}",
        pack.calls.borrow()
    );
}

// -- Components a pack offers and Hlin has no word for --------------------

/// The forwarding case: a name this crate has never heard of reaches the pack.
#[test]
fn a_component_the_pack_offers_is_what_draws() {
    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)).drawn_by(Some("recorder.known")),
        &pack,
        None,
    );

    assert_eq!(
        pack.calls.borrow().as_slice(),
        ["custom"],
        "a component the pack offers should draw instead of the declared kind"
    );
}

/// The safety claim, and the reason naming a component needs no validation
/// anywhere: declining costs the panel nothing.
#[test]
fn a_component_the_pack_declines_falls_back_to_the_declared_kind() {
    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)).drawn_by(Some("nobody.offers.this")),
        &pack,
        None,
    );

    assert_eq!(
        pack.calls.borrow().as_slice(),
        ["stat"],
        "declining a component must draw the declared kind, not nothing"
    );
}

/// Totality has to survive the new path, so this walks the whole matrix again
/// with a component named that no pack offers. Every combination must still
/// draw exactly once.
#[test]
fn naming_an_unknown_component_leaves_rendering_total() {
    for state in PanelState::all() {
        for kind in KINDS {
            for name in ENVELOPES {
                let pack = Recorder::default();
                let document = envelope_named(name);

                hlin_view::draw(
                    &plan(state, kind, Some(&document)).drawn_by(Some("nobody.offers.this")),
                    &pack,
                    Some(30),
                );

                assert_eq!(
                    pack.calls.borrow().len(),
                    1,
                    "{state:?}/{kind}/{name} with an unknown component drew {:?}",
                    pack.calls.borrow()
                );
                assert!(
                    !pack.calls.borrow().contains(&"custom"),
                    "{state:?}/{kind}/{name} drew a component the pack declined"
                );
            }
        }
    }
}

/// A panel that names no component behaves exactly as it did before there was
/// such a thing, which is what makes this backwards compatible rather than
/// merely additive.
#[test]
fn naming_no_component_never_asks_the_pack_about_one() {
    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)),
        &pack,
        None,
    );

    assert_eq!(pack.calls.borrow().as_slice(), ["stat"]);
}

// -- The return path ------------------------------------------------------

/// The emitter a plan carries reaches the pack that draws it, which is the
/// whole mechanism: without this a component's handlers would be wired to
/// nothing and nobody would find out until a browser.
#[test]
fn what_a_component_emits_reaches_the_sink() {
    use std::sync::{Arc, Mutex};

    let heard: Arc<Mutex<Vec<hlin_view::Intent>>> = Arc::new(Mutex::new(Vec::new()));
    let recording = Arc::clone(&heard);

    let scalar = envelope_named("scalar.v1");
    let drawn = plan(PanelState::Ready, Kind::Stat, Some(&scalar)).interactive(
        vec![],
        hlin_view::Emit::to(move |intent| recording.lock().expect("not poisoned").push(intent)),
    );

    // What a pack would do from inside a click handler.
    drawn.emit.send(hlin_view::Intent::Select {
        param: "cluster".to_string(),
        values: vec!["eu-west".to_string()],
    });

    assert_eq!(
        heard.lock().expect("not poisoned").as_slice(),
        [hlin_view::Intent::Select {
            param: "cluster".to_string(),
            values: vec!["eu-west".to_string()],
        }]
    );
}

/// A plan nobody wired still draws, and a component's handlers are still safe
/// to call. This is what lets a pack write one version of itself rather than
/// one for a live surface and one for a test.
#[test]
fn an_unwired_plan_swallows_what_a_component_says() {
    let scalar = envelope_named("scalar.v1");
    let drawn = plan(PanelState::Ready, Kind::Stat, Some(&scalar));

    assert!(!drawn.emit.is_live());
    drawn.emit.send(hlin_view::Intent::Range {
        from_millis: 0,
        to_millis: 1,
    });
}

/// The context a pack is handed carries the controls the plan was given, so a
/// component drawing its own filter reads them from the same place every other
/// fact about the panel comes from.
#[test]
fn controls_reach_the_pack_that_draws() {
    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    let control = hlin_view::Control {
        param: "select".to_string(),
        id: "cluster".to_string(),
        label: "Cluster".to_string(),
        chosen: vec!["eu-west".to_string()],
        choices: vec![],
    };

    let drawn = plan(PanelState::Ready, Kind::Stat, Some(&scalar))
        .interactive(vec![control.clone()], hlin_view::Emit::nowhere());

    assert_eq!(drawn.controls, vec![control]);
    assert_eq!(
        drawn.controls[0].value(),
        Some("eu-west"),
        "a control reports what it is set to"
    );

    // And drawing still works with them present.
    hlin_view::draw(&drawn, &pack, None);
    assert_eq!(pack.calls.borrow().as_slice(), ["stat"]);
}

/// `offers` is advisory and must never gate `custom`.
///
/// A pack that listed a component it then declined would produce a panel
/// nothing draws — precisely the hole the vocabulary exists to prevent. So the
/// two are checked against each other rather than assumed consistent: whatever
/// a pack lists, drawing still goes through `custom` and still falls back.
#[test]
fn offering_a_component_does_not_gate_drawing_it() {
    use hlin_view::DesignPack;

    let pack = Recorder::default();
    let scalar = envelope_named("scalar.v1");

    // Listed and offered: `custom` draws.
    assert!(pack.offers().contains(&"recorder.known"));
    hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)).drawn_by(Some("recorder.known")),
        &pack,
        None,
    );
    assert_eq!(pack.calls.borrow().as_slice(), ["custom"]);

    // Not listed: the declared kind draws, exactly as before `offers` existed.
    let pack = Recorder::default();
    assert!(!pack.offers().contains(&"nobody.offers.this"));
    hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)).drawn_by(Some("nobody.offers.this")),
        &pack,
        None,
    );
    assert_eq!(pack.calls.borrow().as_slice(), ["stat"]);
}

/// A pack that lies — listing a component it declines — must still draw.
///
/// This is the case `offers` could have broken if anything had been allowed to
/// gate on it. Rendering stays total because `custom` remains the authority.
#[test]
fn a_pack_that_lists_what_it_declines_still_draws() {
    struct Liar;

    impl hlin_view::DesignPack for Liar {
        type View = &'static str;
        fn stat(
            &self,
            _: &hlin_manifest::envelope::Scalar,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "stat"
        }
        fn timeseries(
            &self,
            _: &hlin_manifest::envelope::Series,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "timeseries"
        }
        fn sparkline(
            &self,
            _: &hlin_manifest::envelope::Series,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "sparkline"
        }
        fn table(
            &self,
            _: &hlin_manifest::envelope::Records,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "table"
        }
        fn series_as_table(
            &self,
            _: &hlin_manifest::envelope::Series,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "series_as_table"
        }
        fn status(
            &self,
            _: &hlin_manifest::envelope::Status,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "status"
        }
        fn options(
            &self,
            _: &hlin_manifest::envelope::Options,
            _: hlin_view::pack::Context,
        ) -> Self::View {
            "options"
        }
        fn raw(&self, _: &hlin_manifest::Envelope, _: hlin_view::pack::Context) -> Self::View {
            "raw"
        }
        // Lists one and draws none.
        fn offers(&self) -> &'static [&'static str] {
            &["liar.component"]
        }
        fn custom(
            &self,
            _: &str,
            _: &hlin_manifest::Envelope,
            _: hlin_view::pack::Context,
        ) -> Option<Self::View> {
            None
        }
        fn skeleton(&self, _: Kind, _: hlin_view::pack::Context) -> Self::View {
            "skeleton"
        }
        fn placeholder(&self, _: Kind, _: hlin_view::pack::Context) -> Self::View {
            "placeholder"
        }
        fn stylesheet(&self) -> &'static str {
            ""
        }
    }

    let scalar = envelope_named("scalar.v1");
    let drawn = hlin_view::draw(
        &plan(PanelState::Ready, Kind::Stat, Some(&scalar)).drawn_by(Some("liar.component")),
        &Liar,
        None,
    );

    assert_eq!(
        drawn, "stat",
        "a pack that lists a component it declines must still fall back, not leave a hole"
    );
}
