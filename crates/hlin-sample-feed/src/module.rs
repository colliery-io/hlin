//! This platform's own UI module, served as files under its `assets` prefix
//! (specification HLIN-S-0007, *Assets*).
//!
//! The module is built by Trunk from `module/` beside this crate into
//! `module/dist/`, and this serves that directory's files under
//! [`ASSETS`]`posts/`. The shell fetches them from here as itself, with no
//! viewer identity, and serves them to a sandboxed frame from its own origin,
//! so there is nothing to this side but a file server: code is the same for
//! everybody, and who is looking matters only to the API.
//!
//! # Read from disk at start, not embedded
//!
//! The files are read once, when the platform starts, from a directory
//! (`--module-dir`, defaulting to the build output beside this crate) rather
//! than compiled into the binary. Embedding would make the platform binary
//! depend on a WebAssembly build: `cargo check` and `cargo test` of the whole
//! workspace would need Trunk and a finished module first, on every machine
//! and in CI, to test a server that does not care what the module says. Read
//! at start, a checkout builds and tests without Trunk, and a platform started
//! with no module built still serves its manifest; the shell finds the entry
//! missing and draws the `table` fallback, which is what the fallback is for.
//! `angreal demo up --with collab` builds the module before starting this.
//!
//! Held in memory once read, and looked up by exact name, so no path a caller
//! sends ever reaches the filesystem.
//!
//! # Caching
//!
//! Trunk puts a content hash in the name of everything it builds but the
//! entry document, so those files are `immutable`: a new build is a new name.
//! The entry and `boot.js`, whose names never change, are `no-cache`, which
//! the shell would impose on the entry anyway.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Where this platform's module assets live, relative to its base.
pub const ASSETS: &str = "/ui/";

/// The `posts` panel's module: its entry document.
pub const POSTS_ENTRY: &str = "/ui/posts/index.html";

/// Where the module's files are served from, beneath its base.
pub const POSTS_DIR: &str = "/ui/posts/";

/// Where Trunk puts the built module, in this repository.
pub const BUILT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist");

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

/// `GET /ui/posts/{file}`.
pub async fn asset(State(files): State<ModuleFiles>, UrlPath(name): UrlPath<String>) -> Response {
    files.serve(&name)
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
        assert!(is_hashed("hlin-sample-feed-module-ad726f0dc22e8941.js"));
        assert!(is_hashed(
            "hlin-sample-feed-module-ad726f0dc22e8941_bg.wasm"
        ));
        assert!(is_hashed("module-0123456789abcdef.css"));
        assert!(!is_hashed("index.html"));
        assert!(!is_hashed("boot.js"));
        assert!(!is_hashed("hlin-sample-feed-module.js"));
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
    fn a_build_is_read_from_its_directory() {
        let dir = std::env::temp_dir().join(format!("feed-module-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(dir.join("nested/ignored.js"), "").unwrap();

        let files = ModuleFiles::read(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(files.has_entry());
        assert_eq!(files.serve("nested").status(), StatusCode::NOT_FOUND);
    }
}
