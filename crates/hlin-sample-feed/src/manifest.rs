//! The manifest this platform serves about itself.
//!
//! Built as a value so the binary can validate it with `hlin-manifest` before
//! serving it, as the read-only reference does. What is new here is `routes`:
//! the prefixes a module of this platform may call through the shell's request
//! proxy, reads and writes declared apart ([[HLIN-S-0007]]); and `assets`,
//! where that module's own files are ([`crate::module`]).

use hlin_manifest::manifest::{
    Lifecycle, Manifest, ModuleUi, Panel, Platform, Routes, SUPPORTED_SCHEMA_VERSION,
};

/// The contract version this platform declares.
///
/// 1.1.0 adds the module: `assets` and the `posts` panel's `ui`.
pub const CONTRACT_VERSION: &str = "1.1.0";

/// The one panel: the posts.
pub const POSTS_PANEL: &str = "posts";

/// Where the shell-drawn fallback for [`POSTS_PANEL`] reads its data.
pub const POSTS_DATA: &str = "api/panels/posts";

/// Where the event stream is.
pub const EVENTS: &str = "api/events";

/// The prefix modules may read and write under.
///
/// One prefix for both. A module may reach everything under it, which is
/// enough: this platform authorizes every request itself, and does not need
/// protecting from its own module. What is outside it — the manifest — no page
/// can reach through the shell at all.
pub const API_PREFIX: &str = "/api/";

/// Build the manifest for a feed platform of this name.
pub fn build(name: &str) -> Manifest {
    Manifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        contract_version: CONTRACT_VERSION.parse().expect("a valid semver constant"),
        platform: Platform {
            id: name.to_string(),
            name: "Feed".to_string(),
            icon: Some("message".to_string()),
            extra: Default::default(),
        },
        navigation: vec![],
        panels: vec![posts()],
        health: "api/health".to_string(),
        events: Some(EVENTS.to_string()),
        assets: Some(crate::module::ASSETS.to_string()),
        routes: Some(Routes {
            read: vec![API_PREFIX.to_string()],
            write: vec![API_PREFIX.to_string()],
            extra: Default::default(),
        }),
        extra: Default::default(),
    }
}

/// The posts, newest first.
///
/// Drawn by the feed's own module where the shell can host it, and by the
/// shell as a table where it cannot: the module fails to load, stops
/// answering, or the shell has no module host at all.
///
/// `pushed`, because the posts are data this platform keeps and it announces
/// every change on its stream: a post somebody else makes reaches the table
/// as it happens rather than at the next poll.
fn posts() -> Panel {
    Panel {
        key: POSTS_PANEL.to_string(),
        title: "Posts".to_string(),
        description: Some("What people are saying, newest first".to_string()),
        kind: Some("table".to_string()),
        ui: Some(ModuleUi {
            entry: crate::module::POSTS_ENTRY.to_string(),
            bridge: 1,
            extra: Default::default(),
        }),
        envelope: Some("records.v1".to_string()),
        data: Some(POSTS_DATA.to_string()),
        params: vec![],
        refresh_ms: None,
        pushed: true,
        component: None,
        lifecycle: Lifecycle::default(),
        extra: Default::default(),
    }
}
