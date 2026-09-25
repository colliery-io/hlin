//! The converter, as the shell sees it: a module to host, and nothing else.
//!
//! Its rules (the units and the arithmetic) are its module's, and are tested
//! there, in `module/src/units.rs`.

use hlin_widget_converter::{PANEL, api, widget};
use hlin_widget_support::testing::{self, person};

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn it_declares_no_fallback_and_says_why() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind, None);
    assert_eq!(panel.envelope, None);
    assert_eq!(panel.data, None);
    assert!(panel.ui.is_some(), "it is a module and nothing else");
    assert!(!panel.pushed);
    assert!(
        panel
            .description
            .as_deref()
            .is_some_and(|words| words.contains("no fallback")),
        "the panel picker says why there is no fallback"
    );
}

#[test]
fn the_module_makes_no_requests() {
    // The rule is the module's, so it is checked in the module's words: no
    // fetch, streamed or not, and none of the widget kit's calls that make one.
    for (file, source) in [
        ("main.rs", include_str!("../module/src/main.rs")),
        ("units.rs", include_str!("../module/src/units.rs")),
    ] {
        for call in [
            "Request::",
            ".fetch(",
            ".fetch_stream(",
            ".attempt(",
            ".load::",
            ".send(",
            ".send_then(",
        ] {
            assert!(!source.contains(call), "module/src/{file} calls {call}");
        }
    }
}

#[tokio::test]
async fn it_serves_nothing_of_its_own_but_what_every_widget_serves() {
    let converter = testing::start(widget(), (), api()).await;
    let alice = person("u-alice", "Alice");

    assert_eq!(converter.get(&alice, "/api/health").await.status, 200);
    assert_eq!(
        converter
            .get(&alice, &format!("/api/panels/{PANEL}"))
            .await
            .status,
        404,
        "no fallback, so no fallback data"
    );
    assert_eq!(converter.get(&alice, "/api/convert").await.status, 404);
}
