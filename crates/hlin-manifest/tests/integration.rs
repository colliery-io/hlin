//! The manifest specification, as tests.
//!
//! Every requirement in HLIN-S-0001 that this crate is responsible for has a
//! test here, named for the rule rather than the function, so a reader can
//! check the specification against the suite line by line.

use hlin_manifest::manifest::SUPPORTED_SCHEMA_VERSION;
use hlin_manifest::params::ParamDefect;
use hlin_manifest::path::PathDefect;
use hlin_manifest::{
    DocumentDefect, Manifest, PanelDefect, contract_content, contract_hash, parse_str, validate,
};

const SPEC_EXAMPLE: &str = include_str!("fixtures/spec-example.json");

fn manifest_with(panels: &str) -> Manifest {
    parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": {{ "id": "orebank", "name": "Orebank" }},
          "panels": [{panels}],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

fn panel(fields: &str) -> String {
    format!(
        r#"{{ "key": "queue-depth", "title": "Queue depth", "kind": "stat",
             "envelope": "scalar.v1", "data": "api/hlin/queue-depth"{fields} }}"#
    )
}

// -- Reading a document (REQ-1.3, NFR-1.3) ---------------------------------

#[test]
fn the_specifications_example_round_trips() {
    let manifest = parse_str(SPEC_EXAMPLE).expect("parses");
    let reserialized = serde_json::to_string(&manifest).expect("serialises");
    let again = parse_str(&reserialized).expect("re-parses");
    assert_eq!(manifest, again);
}

#[test]
fn unknown_fields_are_preserved_at_every_level() {
    let document = r#"{
      "schema_version": 1,
      "contract_version": "1.0.0",
      "platform": { "id": "orebank", "name": "Orebank", "tier": "gold" },
      "navigation": [{ "label": "Ingest", "path": "ingest", "badge": "new" }],
      "panels": [{
        "key": "queue-depth", "title": "Queue depth", "kind": "stat",
        "envelope": "scalar.v1", "data": "api/hlin/queue-depth",
        "refresh_hint": 30,
        "lifecycle": { "status": "active", "note": "stable since 2024" }
      }],
      "health": "api/health",
      "contact": "platform-team@example.com"
    }"#;

    let manifest = parse_str(document).expect("parses");
    assert_eq!(
        manifest.extra.get("contact").and_then(|v| v.as_str()),
        Some("platform-team@example.com")
    );
    assert!(manifest.platform.extra.contains_key("tier"));
    assert!(manifest.navigation[0].extra.contains_key("badge"));
    assert!(manifest.panels[0].extra.contains_key("refresh_hint"));
    assert!(manifest.panels[0].lifecycle.extra.contains_key("note"));

    let round_tripped = parse_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
    assert_eq!(
        manifest, round_tripped,
        "unknown fields survive a round trip"
    );
}

#[test]
fn both_parameter_forms_parse_to_the_same_declaration() {
    let shorthand = manifest_with(&panel(r#", "params": ["time_range"]"#));
    let object = manifest_with(&panel(r#", "params": [{ "param": "time_range" }]"#));
    assert_eq!(shorthand.panels[0].params, object.panels[0].params);
}

// -- Document-level defects (REQ-1.5, REQ-2.1, REQ-3.1) --------------------

#[test]
fn a_manifest_claiming_another_platform_is_malformed() {
    let manifest = manifest_with("");
    let checked = validate(&manifest, "not-orebank");
    assert_eq!(
        checked.document,
        Some(DocumentDefect::PlatformIdMismatch {
            declared: "orebank".to_string(),
            expected: "not-orebank".to_string(),
        })
    );
    assert!(checked.panels.is_empty(), "no panels are examined");
}

#[test]
fn an_unusable_platform_id_is_malformed() {
    let manifest = parse_str(
        r#"{ "schema_version": 1, "contract_version": "1.0.0",
             "platform": { "id": "Ore Bank", "name": "Orebank" },
             "health": "api/health" }"#,
    )
    .unwrap();
    assert!(matches!(
        validate(&manifest, "Ore Bank").document,
        Some(DocumentDefect::InvalidPlatformId { .. })
    ));
}

#[test]
fn a_health_path_pointing_off_the_platform_is_malformed() {
    let manifest = parse_str(
        r#"{ "schema_version": 1, "contract_version": "1.0.0",
             "platform": { "id": "orebank", "name": "Orebank" },
             "health": "https://elsewhere/health" }"#,
    )
    .unwrap();
    assert!(matches!(
        validate(&manifest, "orebank").document,
        Some(DocumentDefect::InvalidHealthPath {
            defect: PathDefect::AbsoluteUrl,
            ..
        })
    ));
}

#[test]
fn a_newer_schema_version_is_read_rather_than_rejected() {
    let document = SPEC_EXAMPLE.replace(
        r#""schema_version": 1"#,
        &format!(r#""schema_version": {}"#, SUPPORTED_SCHEMA_VERSION + 5),
    );
    let manifest = parse_str(&document).unwrap();
    let checked = validate(&manifest, "orebank");
    assert!(checked.is_document_valid(), "a future version still works");
    assert_eq!(
        checked.newer_schema_version,
        Some(SUPPORTED_SCHEMA_VERSION + 5)
    );
    assert_eq!(checked.accepted_keys().len(), 3);
}

// -- Panel isolation (REQ-3.2) --------------------------------------------

#[test]
fn one_defective_panel_rejects_only_itself() {
    let manifest = manifest_with(
        r#"
        { "key": "good-one", "title": "Good", "kind": "stat",
          "envelope": "scalar.v1", "data": "api/a" },
        { "key": "bad-path", "title": "Bad", "kind": "stat",
          "envelope": "scalar.v1", "data": "https://elsewhere/b" },
        { "key": "good-two", "title": "Good", "kind": "stat",
          "envelope": "scalar.v1", "data": "api/c" }
    "#,
    );
    let checked = validate(&manifest, "orebank");
    assert!(checked.is_document_valid(), "the document itself is fine");
    assert_eq!(checked.accepted_keys(), ["good-one", "good-two"]);
    assert_eq!(checked.rejected().len(), 1);
    assert!(matches!(
        checked.rejected()[0].1,
        PanelDefect::InvalidDataPath {
            defect: PathDefect::AbsoluteUrl,
            ..
        }
    ));
}

#[test]
fn a_contested_key_rejects_every_claimant() {
    let manifest = manifest_with(
        r#"
        { "key": "queue-depth", "title": "First", "kind": "stat",
          "envelope": "scalar.v1", "data": "api/a" },
        { "key": "queue-depth", "title": "Second", "kind": "stat",
          "envelope": "scalar.v1", "data": "api/b" },
        { "key": "untouched", "title": "Other", "kind": "stat",
          "envelope": "scalar.v1", "data": "api/c" }
    "#,
    );
    let checked = validate(&manifest, "orebank");
    assert_eq!(
        checked.accepted_keys(),
        ["untouched"],
        "the shell does not guess which claimant was meant"
    );
    assert_eq!(checked.rejected().len(), 2);
}

#[test]
fn a_deprecated_panel_must_say_when_the_window_ends() {
    let manifest = manifest_with(&panel(r#", "lifecycle": { "status": "deprecated" }"#));
    let checked = validate(&manifest, "orebank");
    assert_eq!(checked.rejected()[0].1, &PanelDefect::MissingSunset);
}

#[test]
fn a_successor_must_be_a_panel_in_this_manifest() {
    let manifest = manifest_with(&panel(
        r#", "lifecycle": { "status": "deprecated", "sunset": "2026-12-01", "successor": "nowhere" }"#,
    ));
    let checked = validate(&manifest, "orebank");
    assert!(matches!(
        checked.rejected()[0].1,
        PanelDefect::UnknownSuccessor { .. }
    ));
}

// -- The parameter vocabulary ---------------------------------------------

#[test]
fn a_platform_cannot_introduce_a_parameter_by_declaring_one() {
    let manifest = manifest_with(&panel(r#", "params": ["tenant"]"#));
    let checked = validate(&manifest, "orebank");
    assert_eq!(
        checked.rejected()[0].1,
        &PanelDefect::InvalidParam(ParamDefect::Unknown {
            param: "tenant".to_string()
        })
    );
}

#[test]
fn a_select_must_carry_its_configuration() {
    let manifest = manifest_with(&panel(
        r#", "params": [{ "param": "select", "id": "cluster" }]"#,
    ));
    let checked = validate(&manifest, "orebank");
    assert_eq!(
        checked.rejected()[0].1,
        &PanelDefect::InvalidParam(ParamDefect::MissingConfig {
            param: "select".to_string(),
            field: "label".to_string()
        })
    );
}

#[test]
fn a_select_cannot_claim_a_reserved_query_name() {
    let manifest = manifest_with(&panel(
        r#", "params": [{ "param": "select", "id": "from", "label": "From", "options": "api/o" }]"#,
    ));
    let checked = validate(&manifest, "orebank");
    assert_eq!(
        checked.rejected()[0].1,
        &PanelDefect::InvalidParam(ParamDefect::ReservedQueryName {
            id: "from".to_string()
        })
    );
}

#[test]
fn one_panel_may_carry_two_selects_but_not_two_of_the_same() {
    let two_selects = panel(
        r#", "params": [
            { "param": "select", "id": "cluster", "label": "Cluster", "options": "api/c" },
            { "param": "select", "id": "region", "label": "Region", "options": "api/r" }
        ]"#,
    );
    assert!(
        validate(&manifest_with(&two_selects), "orebank")
            .rejected()
            .is_empty()
    );

    let repeated = panel(r#", "params": ["time_range", "time_range"]"#);
    assert!(matches!(
        validate(&manifest_with(&repeated), "orebank").rejected()[0].1,
        PanelDefect::DuplicateParam { .. }
    ));
}

// -- Contract identity (REQ-2.2, NFR-1.2) ---------------------------------

#[test]
fn the_fingerprint_ignores_how_the_document_was_written() {
    let manifest = parse_str(SPEC_EXAMPLE).unwrap();
    let compact = parse_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
    assert_eq!(contract_hash(&manifest), contract_hash(&compact));
}

#[test]
fn the_fingerprint_ignores_panel_order_and_parameter_order() {
    let one = manifest_with(
        r#"
        { "key": "alpha", "title": "A", "kind": "stat", "envelope": "scalar.v1",
          "data": "api/a",
          "params": ["time_range", { "param": "select", "id": "cluster", "label": "C", "options": "api/c" }] },
        { "key": "beta", "title": "B", "kind": "stat", "envelope": "scalar.v1", "data": "api/b" }
    "#,
    );
    let other = manifest_with(
        r#"
        { "key": "beta", "title": "B", "kind": "stat", "envelope": "scalar.v1", "data": "api/b" },
        { "key": "alpha", "title": "A", "kind": "stat", "envelope": "scalar.v1",
          "data": "api/a",
          "params": [{ "param": "select", "id": "cluster", "label": "C", "options": "api/c" }, "time_range"] }
    "#,
    );
    assert_eq!(contract_hash(&one), contract_hash(&other));
}

#[test]
fn the_fingerprint_ignores_presentation_and_the_declared_version() {
    let base = parse_str(SPEC_EXAMPLE).unwrap();

    let cases = [
        (r#""kind": "timeseries""#, r#""kind": "table""#),
        (
            r#""title": "Ingest throughput""#,
            r#""title": "Throughput, ingest""#,
        ),
        (
            r#""description": "Records per second across ingest workers""#,
            r#""description": "Something else entirely""#,
        ),
        (
            r#""contract_version": "2.1.0""#,
            r#""contract_version": "9.9.9""#,
        ),
        (r#""label": "Ingest""#, r#""label": "Ingestion""#),
        (r#""health": "api/health""#, r#""health": "api/healthz""#),
        (r#""name": "Orebank""#, r#""name": "Ore Bank""#),
    ];

    for (from, to) in cases {
        assert!(SPEC_EXAMPLE.contains(from), "fixture should contain {from}");
        let changed = parse_str(&SPEC_EXAMPLE.replace(from, to)).unwrap();
        assert_eq!(
            contract_hash(&base),
            contract_hash(&changed),
            "changing {from} should not move the fingerprint"
        );
    }
}

#[test]
fn the_fingerprint_moves_when_the_promise_moves() {
    let base = parse_str(SPEC_EXAMPLE).unwrap();

    let cases = [
        (r#""envelope": "series.v1""#, r#""envelope": "records.v1""#),
        (
            r#""data": "api/hlin/queue-depth""#,
            r#""data": "api/hlin/depth""#,
        ),
        (r#""key": "queue-depth","#, r#""key": "depth-of-queue","#),
        (r#""sunset": "2026-12-01""#, r#""sunset": "2026-11-01""#),
        (r#""params": ["time_range"]"#, r#""params": []"#),
    ];

    for (from, to) in cases {
        assert!(SPEC_EXAMPLE.contains(from), "fixture should contain {from}");
        let changed = parse_str(&SPEC_EXAMPLE.replace(from, to)).unwrap();
        assert_ne!(
            contract_hash(&base),
            contract_hash(&changed),
            "changing {from} should move the fingerprint"
        );
    }
}

#[test]
fn contract_content_materialises_defaults_and_drops_the_rest() {
    let manifest = manifest_with(&panel(r#", "description": "ignored", "refresh": 30"#));
    let content = contract_content(&manifest);
    let panels = content["panels"].as_array().expect("panels array");
    let only = panels[0].as_object().expect("panel object");

    let mut keys: Vec<&str> = only.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["data", "envelope", "key", "lifecycle", "params"]);

    assert_eq!(only["params"], serde_json::json!([]));
    assert_eq!(only["lifecycle"], serde_json::json!({ "status": "active" }));
}

#[test]
fn shorthand_and_object_parameters_fingerprint_alike() {
    let shorthand = manifest_with(&panel(r#", "params": ["time_range"]"#));
    let object = manifest_with(&panel(r#", "params": [{ "param": "time_range" }]"#));
    assert_eq!(contract_hash(&shorthand), contract_hash(&object));
}

#[test]
fn the_fingerprint_is_a_hex_sha256() {
    let manifest = parse_str(SPEC_EXAMPLE).unwrap();
    let hash = contract_hash(&manifest);
    assert_eq!(hash.as_str().len(), 64);
    assert!(hash.as_str().chars().all(|c| c.is_ascii_hexdigit()));
}

// -- Canonicalisation (NFR-1.2) -------------------------------------------

#[test]
fn canonicalisation_follows_rfc_8785() {
    use hlin_manifest::canonical::canonicalize;
    use serde_json::json;

    // Two independent implementations of the shell must agree on a
    // fingerprint, so the canonical form has to be exactly RFC 8785 rather
    // than merely deterministic. These vectors pin the three places
    // implementations usually differ.

    // Numbers use ECMAScript's shortest round-tripping form.
    for (input, expected) in [
        ("-0", "0"),
        ("1e30", "1e+30"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
        ("1e-7", "1e-7"),
        ("0.1", "0.1"),
        ("333333333.33333329", "333333333.3333333"),
    ] {
        let value: serde_json::Value = serde_json::from_str(input).unwrap();
        assert_eq!(canonicalize(&value), expected, "number {input}");
    }

    // Object keys sort by UTF-16 code unit, which is not the same as sorting
    // by UTF-8 byte: a supplementary-plane character begins with a surrogate
    // in the 0xD800 range and so sorts before U+FF3A, while its UTF-8 encoding
    // would sort after.
    let keys: serde_json::Value =
        serde_json::from_str("{\"\\uFF3A\":1,\"\\uD835\\uDC00\":2,\"b\":3,\"A\":4}").unwrap();
    assert_eq!(
        canonicalize(&keys),
        "{\"A\":4,\"b\":3,\"\u{1D400}\":2,\"\u{FF3A}\":1}"
    );

    // Strings use the two-character escapes where they exist and \u00xx
    // otherwise.
    let strings = json!({ "s": "a\u{8}b\u{1f}c\"d" });
    assert_eq!(canonicalize(&strings), r#"{"s":"a\bb\u001fc\"d"}"#);
}

#[test]
fn a_number_in_an_unknown_parameter_field_is_fingerprinted_stably() {
    // Parameter configuration is contract content and preserves fields this
    // crate does not know, so a platform can put a number where the
    // canonicaliser's number rules matter.
    let with_number = |literal: &str| {
        parse_str(&format!(
            r#"{{
              "schema_version": 1, "contract_version": "1.0.0",
              "platform": {{ "id": "orebank", "name": "Orebank" }},
              "panels": [{{
                "key": "queue-depth", "title": "Queue depth", "kind": "stat",
                "envelope": "scalar.v1", "data": "api/q",
                "params": [{{ "param": "select", "id": "cluster", "label": "C",
                              "options": "api/c", "page_size": {literal} }}]
              }}],
              "health": "api/health"
            }}"#
        ))
        .unwrap()
    };

    // The same value written two ways is the same promise.
    assert_eq!(
        contract_hash(&with_number("100")),
        contract_hash(&with_number("1e2"))
    );
    assert_ne!(
        contract_hash(&with_number("100")),
        contract_hash(&with_number("101"))
    );
}

#[test]
fn a_panel_cannot_declare_a_parameter_envelope() {
    // `options.v1` is what a `select` control reads, not what a panel shows.
    // No view kind draws one, so a panel declaring it would be a panel nobody
    // can render meaningfully.
    let manifest = manifest_with(
        r#"{ "key": "clusters", "title": "Clusters", "kind": "table",
             "envelope": "options.v1", "data": "api/hlin/clusters" }"#,
    );
    let checked = validate(&manifest, "orebank");
    assert_eq!(
        checked.rejected()[0].1,
        &PanelDefect::NotAPanelEnvelope {
            declared: "options.v1".to_string()
        }
    );
}

#[test]
fn a_panel_may_declare_an_envelope_this_shell_has_not_learned_yet() {
    // The envelope vocabulary grows at platform speed. A panel naming a newer
    // envelope stays in the picker and fails when its data arrives, rather
    // than disappearing because this shell is behind.
    let manifest = manifest_with(
        r#"{ "key": "latency", "title": "Latency", "kind": "timeseries",
             "envelope": "distribution.v1", "data": "api/hlin/latency" }"#,
    );
    let checked = validate(&manifest, "orebank");
    assert_eq!(checked.accepted_keys(), ["latency"]);
}

// -- Event streams (HLIN-S-0006) -------------------------------------------

#[test]
fn declaring_an_event_stream_is_not_a_contract_change() {
    // The property the whole feature rests on: a platform that adds, moves or
    // withdraws an event stream owes nobody a major version, because the shell
    // was already correct without it. If this ever fails, adopting push has
    // become a breaking change and every consumer has to be told.
    let base = parse_str(SPEC_EXAMPLE).unwrap();

    let with_stream = SPEC_EXAMPLE.replace(
        r#""health": "api/health""#,
        r#""health": "api/health", "events": "api/events""#,
    );
    let with_stream = parse_str(&with_stream).unwrap();
    assert_eq!(with_stream.events.as_deref(), Some("api/events"));
    assert_eq!(
        contract_hash(&base),
        contract_hash(&with_stream),
        "adding an event stream must not move the fingerprint"
    );

    let moved = SPEC_EXAMPLE.replace(
        r#""health": "api/health""#,
        r#""health": "api/health", "events": "api/hlin/events""#,
    );
    assert_eq!(
        contract_hash(&base),
        contract_hash(&parse_str(&moved).unwrap()),
        "moving it must not either"
    );
}

#[test]
fn a_panel_saying_it_is_pushed_is_not_a_contract_change() {
    let base = parse_str(SPEC_EXAMPLE).unwrap();

    let pushed = SPEC_EXAMPLE.replace(
        r#""key": "queue-depth","#,
        r#""key": "queue-depth", "pushed": true,"#,
    );
    let pushed = parse_str(&pushed).unwrap();

    assert!(
        pushed.panel("queue-depth").expect("the panel").pushed,
        "the field parsed"
    );
    assert_eq!(
        contract_hash(&base),
        contract_hash(&pushed),
        "a panel offering to report its own changes has not changed what it promises"
    );
}

#[test]
fn a_manifest_that_says_nothing_about_events_is_unchanged() {
    // REQ-1.1. The overwhelming majority of manifests will never mention any of
    // this, and none of them should behave differently for its existence.
    let manifest = parse_str(SPEC_EXAMPLE).unwrap();

    assert_eq!(manifest.events, None);
    assert!(manifest.panels.iter().all(|panel| !panel.pushed));

    // And round-trips without growing fields it never declared, so a shell
    // re-serialising a manifest does not invent an event stream.
    let round_tripped = serde_json::to_string(&manifest).unwrap();
    assert!(!round_tripped.contains("events"), "{round_tripped}");
    assert!(!round_tripped.contains("pushed"), "{round_tripped}");
}

#[test]
fn an_unusable_events_path_costs_the_stream_and_nothing_else() {
    // A bad `health` path rejects the document, because the shell cannot tell
    // whether the platform is alive. A bad `events` path must not, because the
    // shell is correct without it — rejecting would mean that getting an
    // optional field wrong takes an entire platform offline, which would make
    // the feature more dangerous to adopt than to skip.
    for bad in [
        r#""events": "https://elsewhere.example.com/events""#,
        r#""events": "//elsewhere.example.com/events""#,
        r#""events": "../../events""#,
    ] {
        let source = SPEC_EXAMPLE.replace(
            r#""health": "api/health""#,
            &format!(r#""health": "api/health", {bad}"#),
        );
        let manifest = parse_str(&source).unwrap();
        let outcome = validate(&manifest, "orebank");

        assert!(
            outcome.is_document_valid(),
            "{bad} must not reject the document"
        );
        assert!(
            outcome.unusable_events.is_some(),
            "{bad} should be reported as unusable"
        );
        assert!(
            !outcome.accepted_keys().is_empty(),
            "{bad} must leave every panel exactly as offerable as before"
        );
    }

    // And a usable one says nothing at all.
    let good = SPEC_EXAMPLE.replace(
        r#""health": "api/health""#,
        r#""health": "api/health", "events": "api/events""#,
    );
    let outcome = validate(&parse_str(&good).unwrap(), "orebank");
    assert!(outcome.unusable_events.is_none());
}

#[test]
fn a_panel_may_offer_to_be_pushed_before_its_platform_can_do_it() {
    // A platform mid-adoption: the panels say they will be reported on, the
    // stream is not there yet. Not an error — it is a platform that has not
    // finished, and the shell polls it exactly as it always has.
    let source = SPEC_EXAMPLE.replace(
        r#""key": "queue-depth","#,
        r#""key": "queue-depth", "pushed": true,"#,
    );
    let manifest = parse_str(&source).unwrap();

    assert_eq!(manifest.events, None);
    assert!(manifest.panel("queue-depth").unwrap().pushed);

    let outcome = validate(&manifest, "orebank");
    assert!(outcome.is_document_valid());
    assert!(outcome.unusable_events.is_none());
    assert!(outcome.accepted_keys().contains(&"queue-depth"));
}
