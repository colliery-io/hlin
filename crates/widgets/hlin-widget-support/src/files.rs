//! A widget's built module, served as files under its `assets` prefix
//! ([[HLIN-S-0007]], *Assets*).
//!
//! Trunk builds `module/` beside each widget crate into `module/dist/`, and
//! this serves that directory's files under `/ui/{panel}/`. The shell fetches
//! them as itself, with no viewer identity, and serves them to a sandboxed
//! frame from its own origin, so there is nothing to this side but a file
//! server: code is the same for everybody, and who is looking matters only to
//! the API.
//!
//! The feed's file server (`hlin-sample-feed`), moved here so twenty widgets
//! do not carry twenty copies. The two decisions it made hold for all of them:
//!
//! - **Read from disk at start, not embedded.** Embedding would make every
//!   widget binary depend on a WebAssembly build, so `cargo check` and `cargo
//!   test` of the workspace would need Trunk and twenty finished modules
//!   first. Read at start, a checkout builds and tests without Trunk, and a
//!   widget started with no module built still serves its manifest; the shell
//!   finds the entry missing and draws the fallback, which is what the
//!   fallback is for. Held in memory once read and looked up by exact name, so
//!   no path a caller sends ever reaches the filesystem.
//! - **Hashed names are `immutable`.** Trunk puts a content hash in the name of
//!   everything it builds but the entry document, so a new build is a new
//!   name. The entry and `boot.js`, whose names never change, are `no-cache`,
//!   which the shell would impose on the entry anyway.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The built module's files, by name.
#[derive(Debug, Clone, Default)]
pub struct ModuleFiles {
    files: Arc<HashMap<String, File>>,
}

#[derive(Debug)]
struct File {
    content_type: &'static str,
    immutable: bool,
    body: Bytes,
}

impl ModuleFiles {
    /// No module at all: every asset is 404, and the shell draws the fallback.
    pub fn none() -> Self {
        Self::default()
    }

    /// Every file directly in `dir`, which is how Trunk lays out a build.
    ///
    /// Subdirectories are not served, and neither is a name starting with a
    /// dot: that is where `angreal demo up` keeps the stamp that says which
    /// sources a build came from, which is nobody's business but the build's.
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
            if name.starts_with('.') {
                continue;
            }
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
        assert!(is_hashed("hlin-widget-counter-module-ad726f0dc22e8941.js"));
        assert!(is_hashed(
            "hlin-widget-counter-module-ad726f0dc22e8941_bg.wasm"
        ));
        assert!(is_hashed("module-0123456789abcdef.css"));
        assert!(!is_hashed("index.html"));
        assert!(!is_hashed("boot.js"));
        assert!(!is_hashed("hlin-widget-counter-module.js"));
    }

    #[test]
    fn a_file_is_served_with_its_type_and_its_caching() {
        let files = ModuleFiles::from_files([
            ("index.html".to_string(), b"<!doctype html>".to_vec()),
            (
                "app-ad726f0dc22e8941_bg.wasm".to_string(),
                vec![0, 97, 115, 109],
            ),
        ]);
        let entry = files.serve("index.html");
        assert_eq!(entry.status(), StatusCode::OK);
        assert_eq!(entry.headers()[header::CACHE_CONTROL], "no-cache");
        assert!(
            entry.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );

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
        let files = ModuleFiles::from_files([("index.html".to_string(), vec![])]);
        for asked in ["../Cargo.toml", "missing.js", "", "index.html/"] {
            assert_eq!(
                files.serve(asked).status(),
                StatusCode::NOT_FOUND,
                "{asked}"
            );
        }
        assert_eq!(
            ModuleFiles::none().serve("index.html").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn a_build_is_read_from_its_directory_without_its_subdirectories_or_its_stamp() {
        let dir = std::env::temp_dir().join(format!("widget-module-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(dir.join(".built-from"), "abc").unwrap();
        std::fs::write(dir.join("nested/ignored.js"), "").unwrap();

        let files = ModuleFiles::read(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(files.has_entry());
        assert_eq!(files.serve("nested").status(), StatusCode::NOT_FOUND);
        assert_eq!(files.serve(".built-from").status(), StatusCode::NOT_FOUND);
    }
}
