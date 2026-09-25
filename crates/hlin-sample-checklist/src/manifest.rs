//! The manifest this platform serves about itself.
//!
//! Built as a value so the binary can validate it with `hlin-manifest` before
//! serving it, as `hlin-sample-platform` does, and refuse to start if the
//! shell would reject it.
//!
//! Three things are new here beside the read-only reference. `routes` declares
//! what a module may reach through the shell's request proxy, reads and writes
//! both under `/api/`, and nothing outside them (the manifest, for one) can be
//! addressed from a page. `assets` says where the platform's module files are
//! ([`crate::module`]). And the `items` panel declares both: `ui`, the
//! platform's own module, which the shell mounts in a frame; and a plain
//! `table`, which is what every shell can draw, and what this one draws
//! wherever the module cannot load.

use hlin_manifest::manifest::{
    Lifecycle, Manifest, ModuleUi, Panel, ParamDecl, Platform, Routes, SUPPORTED_SCHEMA_VERSION,
};
use serde_json::{Map, Value};

use crate::changes::{ITEMS, LIST_PARAM};

/// The contract version this platform declares.
///
/// 1.1.0 adds the module: `assets` and the `items` panel's `ui`.
pub const VERSION: &str = "1.1.0";

/// Where a module's reads and writes may go.
pub const API_PREFIX: &str = "/api/";

/// The panel's data endpoint.
pub const ITEMS_DATA: &str = "api/hlin/items";

/// The options endpoint for the `list` selection.
pub const LISTS_OPTIONS: &str = "api/hlin/lists";

/// Build the manifest for a platform of this name.
pub fn build(name: &str) -> Manifest {
    Manifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        contract_version: VERSION.parse().expect("a valid semver constant"),
        platform: Platform {
            id: name.to_string(),
            name: "Checklist".to_string(),
            icon: Some("list".to_string()),
            extra: Default::default(),
        },
        navigation: vec![],
        panels: vec![items()],
        health: "api/health".to_string(),
        events: Some("api/events".to_string()),
        assets: Some(crate::module::ASSETS.to_string()),
        routes: Some(Routes {
            read: vec![API_PREFIX.to_string()],
            write: vec![API_PREFIX.to_string()],
            extra: Default::default(),
        }),
        extra: Default::default(),
    }
}

/// One list's items, as a table.
///
/// The `list` selection's options are the lists the *viewer* belongs to,
/// fetched as them, so the picker never offers a list the platform would
/// refuse to show. That is not an authorization hint to the shell: the shell
/// draws whatever options it is given, and the data endpoint still refuses a
/// list somebody names by hand.
///
/// Drawn by the platform's own module where the shell can host it, and by the
/// shell as a table where it cannot: the module fails to load, stops
/// answering, or the shell has no module host at all.
///
/// Pushed, because a list changes when somebody changes it and at no other
/// time, which is exactly the data a stream is for.
fn items() -> Panel {
    let mut select = Map::new();
    select.insert("id".to_string(), Value::String(LIST_PARAM.to_string()));
    select.insert("label".to_string(), Value::String("List".to_string()));
    select.insert(
        "options".to_string(),
        Value::String(LISTS_OPTIONS.to_string()),
    );

    Panel {
        key: ITEMS.to_string(),
        title: "To do".to_string(),
        description: Some("A shared list; pick which".to_string()),
        kind: Some("table".to_string()),
        ui: Some(ModuleUi {
            entry: crate::module::ITEMS_ENTRY.to_string(),
            bridge: 1,
            extra: Default::default(),
        }),
        envelope: Some("records.v1".to_string()),
        data: Some(ITEMS_DATA.to_string()),
        params: vec![ParamDecl {
            param: "select".to_string(),
            config: select,
        }],
        refresh_ms: None,
        pushed: true,
        component: None,
        lifecycle: Lifecycle::default(),
        extra: Default::default(),
    }
}
