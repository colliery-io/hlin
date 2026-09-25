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
//! the shell would impose on the entry anyway. The hash is written without
//! leading zeros, so it is not always sixteen digits ([`is_hashed`]).
//!
//! Every file has an `ETag` from its content, and a matching `If-None-Match`
//! is answered `304`, so revalidating a `no-cache` file costs a round trip
//! rather than the file (HLIN-T-0088).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
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
    etag: HeaderValue,
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
                    etag: etag(&body),
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

    /// One file's answer to a request with these headers: the file, `304`
    /// if the request already holds it, or 404.
    pub fn serve(&self, name: &str, asked: &HeaderMap) -> Response {
        let Some(file) = self.files.get(name) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let cache = if file.immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        let headers = [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static(file.content_type),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            (header::ETAG, file.etag.clone()),
        ];
        if holds(asked, &file.etag) {
            return (StatusCode::NOT_MODIFIED, headers).into_response();
        }
        (headers, file.body.clone()).into_response()
    }
}

/// `GET /ui/posts/{file}`.
pub async fn asset(
    State(files): State<ModuleFiles>,
    UrlPath(name): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    files.serve(&name, &headers)
}

/// A strong validator for this content: the first 128 bits of its SHA-256.
fn etag(body: &[u8]) -> HeaderValue {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(body);
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    HeaderValue::from_str(&format!("\"{hex}\"")).expect("hex is a valid header value")
}

/// Whether `If-None-Match` names this validator, compared weakly as RFC 9110
/// says it must be.
fn holds(asked: &HeaderMap, etag: &HeaderValue) -> bool {
    let Some(tags) = asked
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let ours = etag.to_str().unwrap_or_default();
    tags.split(',')
        .map(str::trim)
        .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == ours)
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

/// Whether Trunk put a content hash in this name: `{crate}-{hash}.js`,
/// `{crate}-{hash}_bg.wasm`, `module-{hash}.css`.
///
/// The hash is a `u64` in hex with no leading zeros, so up to sixteen digits:
/// one build in sixteen has fifteen. Eight or more, so a short all-hex word at
/// the end of a crate's name (`-feed`) does not pass for one.
fn is_hashed(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let stem = stem.strip_suffix("_bg").unwrap_or(stem);
    stem.rsplit_once('-').is_some_and(|(_, hash)| {
        (8..=16).contains(&hash.len()) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
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
        assert!(is_hashed("module-ef8fd81f1008991.css"));
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
        let entry = files.serve("index.html", &HeaderMap::new());
        assert_eq!(entry.status(), StatusCode::OK);
        assert_eq!(entry.headers()[header::CACHE_CONTROL], "no-cache");
        assert!(
            entry.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );

        let wasm = files.serve("app-ad726f0dc22e8941_bg.wasm", &HeaderMap::new());
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
                files.serve(asked, &HeaderMap::new()).status(),
                StatusCode::NOT_FOUND,
                "{asked}"
            );
        }
        assert_eq!(
            ModuleFiles::none()
                .serve("index.html", &HeaderMap::new())
                .status(),
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
        assert_eq!(
            files.serve("nested", &HeaderMap::new()).status(),
            StatusCode::NOT_FOUND
        );
    }
}
