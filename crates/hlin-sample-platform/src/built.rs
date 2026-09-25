//! This platform's one module built with the SDK, served as files
//! (HLIN-T-0071).
//!
//! Every other module here is hand-written plain JavaScript, compiled into the
//! binary (see [`crate::modules`]). The annotations module is a Leptos app on
//! `hlin-module`, built by Trunk from `module/` beside this crate into
//! `module/dist/`, and this serves that directory's files under
//! [`ANNOTATIONS_DIR`]. It is the same file server the checklist and the feed
//! have, and for the same reasons.
//!
//! # Read from disk at start, not embedded
//!
//! Embedding would make the platform binary depend on a WebAssembly build:
//! `cargo check` and `cargo test` of the workspace would need Trunk and a
//! finished module first, to test a server that does not care what the module
//! says. Read at start (`--module-dir`), a checkout builds and tests without
//! Trunk, and a platform started with no module built still serves its
//! manifest; the shell finds the entry missing and says the panel is
//! unavailable. `angreal demo up` builds the module before starting this.
//!
//! Held in memory once read, and looked up by exact name, so no path a caller
//! sends ever reaches the filesystem.
//!
//! # Caching
//!
//! Trunk puts a content hash in the name of everything it builds but the
//! entry document, so those files are `immutable`: a new build is a new name.
//! The entry and `boot.js`, whose names never change, are `no-cache`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The annotations panel's module: its entry document.
pub const ANNOTATIONS_ENTRY: &str = "/ui/annotations/index.html";

/// Where the module's files are, beneath [`crate::modules::ASSETS`].
pub const ANNOTATIONS_DIR: &str = "annotations/";

/// Where Trunk puts the built module, in this repository.
pub const BUILT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist");

/// The built module's files, by name.
#[derive(Debug, Clone, Default)]
pub struct Built {
    files: Arc<HashMap<String, File>>,
}

#[derive(Debug)]
struct File {
    content_type: &'static str,
    immutable: bool,
    body: Bytes,
}

impl Built {
    /// No module at all: every file is 404, and the panel is unavailable.
    pub fn none() -> Self {
        Self::default()
    }

    /// Every file directly in `dir`, which is how Trunk lays out a build.
    /// Subdirectories are not served; this module has none.
    pub fn read(dir: &Path) -> std::io::Result<Self> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            files.push((name, std::fs::read(entry.path())?));
        }
        Ok(Self::from_files(files))
    }

    /// These files, by name. For a test, or anything that has them already.
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let files = files
            .into_iter()
            .map(|(name, body)| {
                let file = File {
                    content_type: content_type(&name),
                    immutable: is_hashed(&name),
                    body: Bytes::from(body),
                };
                (name, file)
            })
            .collect();
        Self {
            files: Arc::new(files),
        }
    }

    /// Whether there is an entry document to serve.
    pub fn has_entry(&self) -> bool {
        self.files.contains_key("index.html")
    }

    /// One file's answer, or 404.
    pub fn serve(&self, name: &str) -> Response {
        let Some(file) = self.files.get(name) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let cache = if file.immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(file.content_type),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            ],
            file.body.clone(),
        )
            .into_response()
    }
}

/// The type a file is served as. The shell insists on `application/wasm` and
/// `text/html` whatever is said here; the rest it takes from the platform.
fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Whether Trunk put a content hash in this name: `{crate}-{16 hex}.js`,
/// `{crate}-{16 hex}_bg.wasm`, `module-{16 hex}.css`.
fn is_hashed(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let stem = stem.strip_suffix("_bg").unwrap_or(stem);
    stem.rsplit_once('-').is_some_and(|(_, hash)| {
        hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunks_hashed_output_is_immutable_and_the_names_that_never_change_are_not() {
        assert!(is_hashed("hlin-sample-platform-module-ad726f0dc22e8941.js"));
        assert!(is_hashed(
            "hlin-sample-platform-module-ad726f0dc22e8941_bg.wasm"
        ));
        assert!(is_hashed("module-0123456789abcdef.css"));
        assert!(!is_hashed("index.html"));
        assert!(!is_hashed("boot.js"));
    }

    #[test]
    fn a_file_is_served_with_its_type_and_its_caching() {
        let files = Built::from_files([
            ("index.html".to_string(), b"<!doctype html>".to_vec()),
            (
                "app-ad726f0dc22e8941_bg.wasm".to_string(),
                vec![0, 97, 115, 109],
            ),
        ]);
        let entry = files.serve("index.html");
        assert_eq!(entry.status(), StatusCode::OK);
        assert_eq!(entry.headers()[header::CACHE_CONTROL], "no-cache");

        let wasm = files.serve("app-ad726f0dc22e8941_bg.wasm");
        assert_eq!(wasm.headers()[header::CONTENT_TYPE], "application/wasm");
        assert!(
            wasm.headers()[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("immutable")
        );
    }

    #[test]
    fn anything_not_in_the_build_is_not_found() {
        let files = Built::from_files([("index.html".to_string(), vec![])]);
        for asked in ["../Cargo.toml", "missing.js", "", "index.html/"] {
            assert_eq!(
                files.serve(asked).status(),
                StatusCode::NOT_FOUND,
                "{asked}"
            );
        }
        assert_eq!(
            Built::none().serve("index.html").status(),
            StatusCode::NOT_FOUND
        );
    }
}
