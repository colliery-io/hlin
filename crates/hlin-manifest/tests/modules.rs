//! Modules in the manifest, as tests.
//!
//! A platform may ship its own UI for a panel or a navigation entry
//! (decision HLIN-A-0014). The manifest says where that code lives, which
//! routes it may call, and which bridge it speaks; the shell decides whether it
//! can host it. These tests walk the fields specification HLIN-S-0001 gained
//! for that, and the rules specification HLIN-S-0007 relies on them for.

use hlin_manifest::path::PrefixDefect;
use hlin_manifest::{
    Access, Change, Class, Manifest, ModuleDefect, ModuleSite, PanelDefect,
    SUPPORTED_BRIDGE_MAJORS, Verdict, classify_diff, contract_content, contract_hash, parse_str,
    validate,
};
use serde_json::json;

const SPEC_EXAMPLE: &str = include_str!("fixtures/spec-example.json");
const MODULE_EXAMPLE: &str = include_str!("fixtures/module-example.json");

/// A platform with the given top-level module fields and panels.
fn platform(fields: &str, panels: &str) -> Manifest {
    parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": {{ "id": "checklist", "name": "Checklist" }},
          {fields}
          "panels": [{panels}],
          "health": "health"
        }}"#
    ))
    .expect("fixture parses")
}

const ASSETS: &str = r#""assets": "/ui/","#;

const MODULE_ONLY: &str = r#"
    { "key": "board", "title": "Board",
      "ui": { "entry": "/ui/board/index.html", "bridge": 1 } }
"#;

const BOTH: &str = r#"
    { "key": "items", "title": "To do",
      "ui": { "entry": "/ui/items/index.html", "bridge": 1 },
      "kind": "table", "envelope": "records.v1", "data": "panels/items" }
"#;

fn panel_defect(manifest: &Manifest, key: &str) -> Option<PanelDefect> {
    validate(manifest, "checklist")
        .panels
        .into_iter()
        .find(|outcome| outcome.key == key)
        .expect("the panel was examined")
        .defect
}

fn module_defect(manifest: &Manifest, key: &str) -> Option<ModuleDefect> {
    validate(manifest, "checklist")
        .panels
        .into_iter()
        .find(|outcome| outcome.key == key)
        .expect("the panel was examined")
        .module
}

// -- Reading (NFR-1.3) ----------------------------------------------------

#[test]
fn the_module_example_reads_and_every_panel_is_accepted() {
    let manifest = parse_str(MODULE_EXAMPLE).expect("parses");
    let checked = validate(&manifest, "checklist");

    assert!(checked.is_document_valid(), "{:?}", checked.document);
    assert_eq!(checked.accepted_keys(), ["items", "board", "open-count"]);
    assert!(
        checked
            .panels
            .iter()
            .all(|outcome| outcome.module.is_none())
    );
    assert!(checked.unusable_assets.is_none());
    assert!(checked.unusable_routes.is_empty());
    assert!(checked.rejected_navigation_modules.is_empty());

    assert_eq!(manifest.assets.as_deref(), Some("/ui/"));
    let routes = manifest.routes.as_ref().expect("routes");
    assert_eq!(routes.prefixes(Access::Read), ["/api/"]);
    assert_eq!(routes.prefixes(Access::Write), ["/api/"]);
    let navigation = manifest.navigation[0].ui.as_ref().expect("a module");
    assert_eq!(navigation.entry, "/ui/lists/index.html");
    assert_eq!(navigation.bridge, 1);
}

#[test]
fn the_module_example_round_trips() {
    let manifest = parse_str(MODULE_EXAMPLE).expect("parses");
    let reserialized = serde_json::to_string(&manifest).expect("serialises");
    assert_eq!(parse_str(&reserialized).expect("re-parses"), manifest);
}

#[test]
fn a_module_only_panel_does_not_grow_data_fields_when_written_back() {
    let manifest = platform(ASSETS, MODULE_ONLY);
    let written = serde_json::to_value(&manifest.panels[0]).expect("serialises");
    let object = written.as_object().expect("an object");
    for absent in ["kind", "envelope", "data"] {
        assert!(!object.contains_key(absent), "{absent} appeared: {written}");
    }
}

#[test]
fn unknown_fields_inside_module_declarations_are_preserved() {
    let manifest = platform(
        r#""assets": "/ui/", "routes": { "read": ["/api/"], "stream": ["/sse/"] },"#,
        r#"{ "key": "board", "title": "Board",
             "ui": { "entry": "/ui/board/index.html", "bridge": 1, "sri": "sha384-x" } }"#,
    );
    let reserialized = serde_json::to_value(&manifest).expect("serialises");
    assert_eq!(reserialized["routes"]["stream"], json!(["/sse/"]));
    assert_eq!(reserialized["panels"][0]["ui"]["sri"], json!("sha384-x"));
}

#[test]
fn a_manifest_that_says_nothing_about_modules_is_unchanged() {
    // The adoption story again: none of the manifests written before modules
    // existed may read, validate, write back or fingerprint any differently.
    let manifest = parse_str(SPEC_EXAMPLE).expect("parses");
    assert_eq!(manifest.assets, None);
    assert_eq!(manifest.routes, None);
    assert!(manifest.panels.iter().all(|panel| panel.ui.is_none()));
    assert!(
        manifest
            .panels
            .iter()
            .all(|panel| panel.drawn_by_shell().is_some())
    );

    let checked = validate(&manifest, "orebank");
    assert!(checked.rejected().is_empty());
    assert!(checked.unusable_assets.is_none());
    assert!(checked.unusable_routes.is_empty());

    let written = serde_json::to_string(&manifest).expect("serialises");
    for absent in ["assets", "routes", "\"ui\""] {
        assert!(!written.contains(absent), "{absent} appeared: {written}");
    }

    // Pinned from before modules existed. If this moves, every platform's
    // contract appears to change the moment a shell upgrades.
    assert_eq!(
        contract_hash(&manifest).as_str(),
        "29494ba4b7600d2a49ca2f4dac6b9c7bc3b6083ce6728fa0de19ee5944e73d9d"
    );
}

// -- What a panel must declare ---------------------------------------------

#[test]
fn a_panel_may_be_drawn_by_its_module_alone() {
    let manifest = platform(ASSETS, MODULE_ONLY);
    assert_eq!(panel_defect(&manifest, "board"), None);
    assert_eq!(manifest.panels[0].drawn_by_shell(), None);
}

#[test]
fn a_panel_may_be_drawn_by_the_shell_alone_or_by_both() {
    let manifest = platform(
        ASSETS,
        &format!(
            r#"{BOTH}, {{ "key": "count", "title": "Count", "kind": "stat",
                 "envelope": "scalar.v1", "data": "panels/count" }}"#
        ),
    );
    assert_eq!(
        validate(&manifest, "checklist").accepted_keys(),
        ["items", "count"]
    );
}

#[test]
fn a_panel_with_nothing_to_draw_it_is_refused_alone() {
    let manifest = platform(
        ASSETS,
        &format!(r#"{{ "key": "empty", "title": "Empty" }}, {MODULE_ONLY}"#),
    );
    let checked = validate(&manifest, "checklist");
    assert!(
        checked.is_document_valid(),
        "the panel, not the manifest, is refused"
    );
    assert_eq!(
        panel_defect(&manifest, "empty"),
        Some(PanelDefect::NothingToDraw)
    );
    assert_eq!(checked.accepted_keys(), ["board"]);
}

#[test]
fn a_panel_without_a_module_still_needs_all_of_its_data() {
    let manifest = platform(
        "",
        r#"{ "key": "count", "title": "Count", "kind": "stat", "envelope": "scalar.v1" }"#,
    );
    assert_eq!(
        panel_defect(&manifest, "count"),
        Some(PanelDefect::MissingField { field: "data" })
    );
}

#[test]
fn a_fallback_is_declared_whole_or_not_at_all() {
    // `kind`, `envelope` and `data` are optional only together. A module's
    // fallback missing its envelope is not a fallback the shell can draw.
    let manifest = platform(
        ASSETS,
        r#"{ "key": "items", "title": "To do",
             "ui": { "entry": "/ui/items/index.html", "bridge": 1 },
             "kind": "table", "data": "panels/items" }"#,
    );
    assert_eq!(
        panel_defect(&manifest, "items"),
        Some(PanelDefect::MissingField { field: "envelope" })
    );
}

// -- Where modules may load from and reach ---------------------------------

#[test]
fn a_module_needs_its_platform_to_declare_assets() {
    let manifest = platform("", &format!("{MODULE_ONLY}, {BOTH}"));
    assert_eq!(
        panel_defect(&manifest, "board"),
        Some(PanelDefect::Module(ModuleDefect::NoAssets))
    );
    // A panel that can also be drawn by the shell keeps its place, and the
    // shell draws it: the fallback HLIN-S-0007 promises for a malformed module.
    assert_eq!(panel_defect(&manifest, "items"), None);
    assert_eq!(
        module_defect(&manifest, "items"),
        Some(ModuleDefect::NoAssets)
    );
}

#[test]
fn a_module_entry_falls_under_the_assets_prefix_by_segment() {
    let manifest = platform(
        ASSETS,
        r#"{ "key": "board", "title": "Board",
             "ui": { "entry": "/uix/board/index.html", "bridge": 1 } }"#,
    );
    assert_eq!(
        panel_defect(&manifest, "board"),
        Some(PanelDefect::Module(ModuleDefect::EntryOutsideAssets {
            entry: "/uix/board/index.html".to_string(),
            assets: "/ui/".to_string(),
        }))
    );
}

#[test]
fn a_module_entry_is_a_rooted_file_path() {
    for (entry, defect) in [
        ("ui/board/index.html", PrefixDefect::NotRooted),
        ("/ui/board/", PrefixDefect::NotAFile),
        ("/ui/../admin/index.html", PrefixDefect::DotSegment),
        (
            "/ui/%2e%2e/admin/index.html",
            PrefixDefect::EncodedSeparator,
        ),
        (
            "https://elsewhere.example.com/ui/index.html",
            PrefixDefect::NotRooted,
        ),
    ] {
        let manifest = platform(
            ASSETS,
            &format!(
                r#"{{ "key": "board", "title": "Board",
                     "ui": {{ "entry": "{entry}", "bridge": 1 }} }}"#
            ),
        );
        assert_eq!(
            panel_defect(&manifest, "board"),
            Some(PanelDefect::Module(ModuleDefect::InvalidEntry {
                declared: entry.to_string(),
                defect,
            })),
            "{entry}"
        );
    }
}

#[test]
fn an_unsupported_bridge_major_is_refused_and_the_supported_ones_named() {
    let manifest = platform(
        ASSETS,
        r#"{ "key": "board", "title": "Board",
             "ui": { "entry": "/ui/board/index.html", "bridge": 2 } }"#,
    );
    let defect = panel_defect(&manifest, "board").expect("refused");
    assert_eq!(
        defect,
        PanelDefect::Module(ModuleDefect::UnsupportedBridge {
            declared: 2,
            supported: SUPPORTED_BRIDGE_MAJORS.to_vec(),
        })
    );
    assert!(
        defect.to_string().contains("supports 1"),
        "the message names what is supported: {defect}"
    );
}

#[test]
fn an_unusable_assets_prefix_costs_the_modules_and_nothing_else() {
    for (assets, expected) in [
        ("ui/", PrefixDefect::NotRooted),
        ("/ui", PrefixDefect::NotSegmentShaped),
        ("/", PrefixDefect::WholePlatform),
        ("https://cdn.example.com/ui/", PrefixDefect::NotRooted),
        ("//cdn.example.com/ui/", PrefixDefect::EmptySegment),
        ("/ui/../", PrefixDefect::DotSegment),
        ("/ui\\x/", PrefixDefect::Backslash),
        ("/ui%2Fx/", PrefixDefect::EncodedSeparator),
    ] {
        let manifest = platform(
            &format!(r#""assets": {},"#, json!(assets)),
            &format!(
                r#"{BOTH}, {{ "key": "count", "title": "Count", "kind": "stat",
                     "envelope": "scalar.v1", "data": "panels/count" }}"#
            ),
        );
        let checked = validate(&manifest, "checklist");
        assert!(checked.is_document_valid(), "{assets}");
        assert_eq!(checked.unusable_assets, Some(expected), "{assets}");
        assert_eq!(checked.accepted_keys(), ["items", "count"], "{assets}");
        assert_eq!(
            module_defect(&manifest, "items"),
            Some(ModuleDefect::NoAssets),
            "{assets}"
        );
    }
}

#[test]
fn an_unusable_route_prefix_costs_that_prefix_alone() {
    let manifest = platform(
        r#""assets": "/ui/",
           "routes": { "read": ["/api/", "/api/../admin/"], "write": ["api/"] },"#,
        MODULE_ONLY,
    );
    let checked = validate(&manifest, "checklist");
    assert!(checked.is_document_valid());
    assert_eq!(checked.accepted_keys(), ["board"]);

    let unusable: Vec<(Access, &str, &PrefixDefect)> = checked
        .unusable_routes
        .iter()
        .map(|route| (route.access, route.prefix.as_str(), &route.defect))
        .collect();
    assert_eq!(
        unusable,
        [
            (Access::Read, "/api/../admin/", &PrefixDefect::DotSegment),
            (Access::Write, "api/", &PrefixDefect::NotRooted),
        ]
    );
}

#[test]
fn a_navigation_module_that_cannot_be_hosted_leaves_the_link() {
    let manifest = parse_str(
        r#"{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": { "id": "checklist", "name": "Checklist" },
          "navigation": [
            { "label": "Lists", "path": "lists",
              "ui": { "entry": "/ui/lists/index.html", "bridge": 1 } },
            { "label": "Settings", "path": "settings" }
          ],
          "health": "health"
        }"#,
    )
    .expect("parses");
    let checked = validate(&manifest, "checklist");
    assert!(checked.is_document_valid());
    assert_eq!(checked.rejected_navigation_modules.len(), 1);
    let rejected = &checked.rejected_navigation_modules[0];
    assert_eq!(
        (rejected.index, rejected.path.as_str(), &rejected.defect),
        (0, "lists", &ModuleDefect::NoAssets)
    );
}

// -- Contract identity -------------------------------------------------------

#[test]
fn modules_and_where_they_reach_are_contract_content() {
    let manifest = platform(
        r#""assets": "/ui/",
           "routes": { "write": ["/api/", "/api/"], "read": ["/files/", "/api/"] },"#,
        MODULE_ONLY,
    );
    assert_eq!(
        contract_content(&manifest),
        json!({
            "assets": "/ui/",
            "routes": { "read": ["/api/", "/files/"], "write": ["/api/"] },
            "panels": [{
                "key": "board",
                "ui": { "entry": "/ui/board/index.html", "bridge": 1 },
                "params": [],
                "lifecycle": { "status": "active" }
            }]
        })
    );
}

#[test]
fn a_navigation_module_is_contract_though_its_entry_is_not() {
    let manifest = parse_str(MODULE_EXAMPLE).expect("parses");
    let content = contract_content(&manifest);
    assert_eq!(
        content["navigation"],
        json!([{ "path": "lists", "ui": { "entry": "/ui/lists/index.html", "bridge": 1 } }])
    );

    // Relabelling the entry is still presentation.
    let relabelled =
        parse_str(&MODULE_EXAMPLE.replace(r#""label": "Lists""#, r#""label": "My lists""#))
            .expect("parses");
    assert_eq!(contract_hash(&manifest), contract_hash(&relabelled));
}

#[test]
fn the_fingerprint_ignores_how_routes_are_written() {
    let one = platform(
        r#""assets": "/ui/", "routes": { "read": ["/a/", "/b/"] },"#,
        MODULE_ONLY,
    );
    let other = platform(
        r#""assets": "/ui/", "routes": { "read": ["/b/", "/a/", "/a/"], "write": [] },"#,
        MODULE_ONLY,
    );
    assert_eq!(contract_hash(&one), contract_hash(&other));
}

#[test]
fn the_fingerprint_moves_when_a_module_moves() {
    let base = parse_str(MODULE_EXAMPLE).expect("parses");
    for (from, to) in [
        (r#""assets": "/ui/""#, r#""assets": "/static/""#),
        (r#""write": ["/api/"]"#, r#""write": []"#),
        (
            r#""entry": "/ui/board/index.html""#,
            r#""entry": "/ui/board/main.html""#,
        ),
        (
            r#""entry": "/ui/lists/index.html", "bridge": 1"#,
            r#""entry": "/ui/lists/index.html", "bridge": 2"#,
        ),
    ] {
        let moved = parse_str(&MODULE_EXAMPLE.replace(from, to)).expect("parses");
        assert_ne!(contract_hash(&base), contract_hash(&moved), "{to}");
    }
}

// -- Diff classification -----------------------------------------------------

fn at(version: &str, source: &str) -> Manifest {
    parse_str(&source.replace(
        r#""contract_version": "1.0.0""#,
        &format!(r#""contract_version": "{version}""#),
    ))
    .expect("parses")
}

fn changes_between(from: &str, to: &str) -> (Vec<Change>, Class, Verdict) {
    let before = at("1.0.0", MODULE_EXAMPLE);
    let after = at("1.1.0", &MODULE_EXAMPLE.replace(from, to));
    let report = classify_diff(&before, &after);
    (report.changes, report.class, report.verdict)
}

fn items() -> ModuleSite {
    ModuleSite::Panel {
        key: "items".to_string(),
    }
}

#[test]
fn adding_modules_and_the_fields_they_need_is_additive() {
    let before = at("1.0.0", SPEC_EXAMPLE);
    let after = at(
        "1.0.0",
        &SPEC_EXAMPLE
            .replace(
                r#""health": "api/health""#,
                r#""health": "api/health", "assets": "/ui/", "routes": { "read": ["/api/"] }"#,
            )
            .replace(
                r#""key": "queue-depth","#,
                r#""key": "queue-depth", "ui": { "entry": "/ui/depth/index.html", "bridge": 1 },"#,
            ),
    );
    let report = classify_diff(&before, &after);
    assert_eq!(report.class, Class::Additive, "{:?}", report.changes);
    assert_eq!(report.verdict, Verdict::Accepted);
    assert!(report.changes.contains(&Change::AssetsChanged {
        from: None,
        to: Some("/ui/".to_string()),
    }));
    assert!(report.changes.contains(&Change::RoutePrefixAdded {
        access: Access::Read,
        prefix: "/api/".to_string(),
    }));
    assert!(report.changes.contains(&Change::ModuleAdded {
        site: ModuleSite::Panel {
            key: "queue-depth".to_string()
        },
    }));
}

#[test]
fn removing_a_panels_module_is_breaking() {
    let (changes, class, verdict) = changes_between(
        r#""ui": { "entry": "/ui/items/index.html", "bridge": 1 },"#,
        "",
    );
    assert_eq!(changes, [Change::ModuleRemoved { site: items() }]);
    assert_eq!(class, Class::Breaking);
    assert!(matches!(
        verdict,
        Verdict::Violation {
            expected_major: 2,
            ..
        }
    ));
}

#[test]
fn changing_a_bridge_major_is_breaking_and_says_so() {
    let (changes, class, _) = changes_between(
        r#""entry": "/ui/items/index.html", "bridge": 1"#,
        r#""entry": "/ui/items/index.html", "bridge": 2"#,
    );
    assert_eq!(
        changes,
        [Change::BridgeChanged {
            site: items(),
            from: 1,
            to: 2
        }]
    );
    assert_eq!(class, Class::Breaking);
}

#[test]
fn moving_a_module_entry_is_breaking_as_moving_data_is() {
    let (changes, class, _) = changes_between(
        r#""entry": "/ui/items/index.html""#,
        r#""entry": "/ui/items/main.html""#,
    );
    assert_eq!(
        changes,
        [Change::ModuleEntryChanged {
            site: items(),
            from: "/ui/items/index.html".to_string(),
            to: "/ui/items/main.html".to_string(),
        }]
    );
    assert_eq!(class, Class::Breaking);
}

#[test]
fn a_navigation_module_is_diffed_by_the_path_it_links_to() {
    let (changes, class, _) = changes_between(
        r#", "ui": { "entry": "/ui/lists/index.html", "bridge": 1 }"#,
        "",
    );
    assert!(changes.contains(&Change::ModuleRemoved {
        site: ModuleSite::Navigation {
            path: "lists".to_string()
        },
    }));
    assert!(changes.contains(&Change::NavigationChanged));
    assert_eq!(class, Class::Breaking);
}

#[test]
fn narrowing_or_moving_the_assets_prefix_is_breaking_and_widening_is_not() {
    let narrowed = Change::AssetsChanged {
        from: Some("/ui/".to_string()),
        to: Some("/ui/items/".to_string()),
    };
    let moved = Change::AssetsChanged {
        from: Some("/ui/".to_string()),
        to: Some("/static/".to_string()),
    };
    let withdrawn = Change::AssetsChanged {
        from: Some("/ui/".to_string()),
        to: None,
    };
    let widened = Change::AssetsChanged {
        from: Some("/ui/items/".to_string()),
        to: Some("/ui/".to_string()),
    };
    assert_eq!(narrowed.class(), Class::Breaking);
    assert_eq!(moved.class(), Class::Breaking);
    assert_eq!(withdrawn.class(), Class::Breaking);
    assert_eq!(widened.class(), Class::Additive);
}

#[test]
fn removing_a_route_prefix_is_breaking_unless_another_still_covers_it() {
    let (changes, class, _) = changes_between(r#""write": ["/api/"]"#, r#""write": []"#);
    assert_eq!(
        changes,
        [Change::RoutePrefixRemoved {
            access: Access::Write,
            prefix: "/api/".to_string(),
            covered_by: None,
        }]
    );
    assert_eq!(class, Class::Breaking);

    let (changes, class, _) = changes_between(r#""read": ["/api/"]"#, r#""read": ["/api/lists/"]"#);
    assert!(changes.contains(&Change::RoutePrefixRemoved {
        access: Access::Read,
        prefix: "/api/".to_string(),
        covered_by: None,
    }));
    assert_eq!(
        class,
        Class::Breaking,
        "narrowing is removing, for a module"
    );

    let before = at(
        "1.0.0",
        &MODULE_EXAMPLE.replace(r#""read": ["/api/"]"#, r#""read": ["/api/lists/"]"#),
    );
    let after = at("1.0.0", MODULE_EXAMPLE);
    let report = classify_diff(&before, &after);
    assert!(report.changes.contains(&Change::RoutePrefixRemoved {
        access: Access::Read,
        prefix: "/api/lists/".to_string(),
        covered_by: Some("/api/".to_string()),
    }));
    assert_eq!(report.class, Class::Additive, "widening withdraws nothing");
}

#[test]
fn dropping_a_modules_fallback_is_breaking_and_adding_one_is_not() {
    let (changes, class, _) = changes_between(
        r#""kind": "table",
      "envelope": "records.v1",
      "data": "/panels/items","#,
        "",
    );
    assert_eq!(
        changes,
        [Change::DataRemoved {
            key: "items".to_string()
        }]
    );
    assert_eq!(class, Class::Breaking);

    let (changes, class, _) = changes_between(
        r#""ui": { "entry": "/ui/board/index.html", "bridge": 1 }"#,
        r#""ui": { "entry": "/ui/board/index.html", "bridge": 1 },
           "kind": "table", "envelope": "records.v1", "data": "/panels/board""#,
    );
    assert_eq!(
        changes,
        [Change::DataAdded {
            key: "board".to_string()
        }]
    );
    assert_eq!(class, Class::Additive);
}
