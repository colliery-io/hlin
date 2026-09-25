//! This platform's own UI modules, served as files (specification HLIN-S-0007).
//!
//! A platform ships a module as ordinary static assets under one prefix, and
//! the shell fetches them from here as itself and serves them to a sandboxed
//! frame from its own origin. So there is nothing to this side but a file
//! server: no identity, because the shell asks without one (code is the same
//! for every viewer), and no knowledge of the bridge, which is between the
//! module and the shell's page.
//!
//! The modules here are hand-written HTML and plain JavaScript rather than
//! built with the SDK, on purpose: they are what the shell's frame host is
//! tested against, and a test double written from the specification catches a
//! page that disagrees with it, where one built from the same crate as the
//! page would agree with any mistake the two shared. One of them is hostile
//! (`hostile/`): it tries every way out of its frame the specification closes,
//! for the browser tests' containment matrix.
//!
//! The one module that *is* built with the SDK, the annotations panel's, is
//! served from its build output beside these (see [`crate::built`]).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Where this platform's module assets live, relative to its base.
pub const ASSETS: &str = "/ui/";

/// The routes its modules may read through the shell.
pub const READS: &str = "/api/module/";

/// A module that says `ready`, answers heartbeats until told not to, and asks
/// its platform who is looking.
pub const PROBE: &str = "/ui/probe/index.html";

/// A module that loads and never says `ready`.
pub const SILENT: &str = "/ui/silent/index.html";

/// A navigation entry's module, which the shell opens as a page at full width.
pub const PAGE: &str = "/ui/page/index.html";

/// A module that tries to escape its frame, and reports what happened.
pub const HOSTILE: &str = "/ui/hostile/index.html";

/// Where the browser tests point the hostile module's navigations and
/// scripts: a page and a script on this platform, reachable from a browser,
/// that a module must never be able to load.
pub const LURE: &str = "/ui/lure/index.html";

/// Every file under [`ASSETS`], by its path beneath it.
const FILES: [(&str, &str, &str); 9] = [
    (
        "probe/index.html",
        "text/html; charset=utf-8",
        include_str!("../ui/probe/index.html"),
    ),
    (
        "probe/probe.js",
        "text/javascript; charset=utf-8",
        include_str!("../ui/probe/probe.js"),
    ),
    (
        "silent/index.html",
        "text/html; charset=utf-8",
        include_str!("../ui/silent/index.html"),
    ),
    (
        "page/index.html",
        "text/html; charset=utf-8",
        include_str!("../ui/page/index.html"),
    ),
    (
        "page/page.js",
        "text/javascript; charset=utf-8",
        include_str!("../ui/page/page.js"),
    ),
    (
        "hostile/index.html",
        "text/html; charset=utf-8",
        include_str!("../ui/hostile/index.html"),
    ),
    (
        "hostile/hostile.js",
        "text/javascript; charset=utf-8",
        include_str!("../ui/hostile/hostile.js"),
    ),
    (
        "lure/index.html",
        "text/html; charset=utf-8",
        include_str!("../ui/lure/index.html"),
    ),
    (
        "lure/lure.js",
        "text/javascript; charset=utf-8",
        include_str!("../ui/lure/lure.js"),
    ),
];

/// `GET /ui/{path}`: one module file, or 404.
///
/// Looked up in a fixed table, or the built module's files read at start,
/// rather than read from disk, so no path a caller sends can name anything
/// but these files.
pub async fn asset(State(config): State<Arc<crate::Config>>, Path(path): Path<String>) -> Response {
    if let Some(name) = path.strip_prefix(crate::built::ANNOTATIONS_DIR) {
        return config.built.serve(name);
    }
    match FILES.iter().find(|(name, _, _)| *name == path) {
        Some((_, content_type, body)) => {
            ([(header::CONTENT_TYPE, *content_type)], *body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_a_panel_names_is_served() {
        for entry in [PROBE, SILENT, PAGE, HOSTILE, LURE] {
            let beneath = entry.strip_prefix(ASSETS).expect("under the prefix");
            assert!(
                FILES.iter().any(|(name, _, _)| *name == beneath),
                "{entry} is declared and not served"
            );
        }
    }

    #[test]
    fn the_probe_loads_its_script_from_a_file_the_csp_allows() {
        // Inline script is refused by the module CSP, so the probe's page must
        // name its script as a file, beside it, that this platform serves.
        let page = FILES[0].2;
        assert!(page.contains(r#"<script src="probe.js"></script>"#));
        assert!(!page.contains("<script>"));
    }

    #[test]
    fn every_hand_written_page_loads_its_script_from_a_file_beside_it() {
        for (name, _, body) in FILES.iter().filter(|(name, _, _)| name.ends_with(".html")) {
            assert!(!body.contains("<script>"), "{name} has inline script");
        }
    }

    #[test]
    fn the_page_loads_its_script_from_a_file_the_csp_allows() {
        let page = FILES
            .iter()
            .find(|(name, _, _)| *name == "page/index.html")
            .expect("the page is served")
            .2;
        assert!(page.contains(r#"<script src="page.js"></script>"#));
        assert!(!page.contains("<script>"));
    }
}
