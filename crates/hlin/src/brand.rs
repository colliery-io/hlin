//! Whose name the shell wears: `[brand]` (COLLIERY-I-0609).
//!
//! An operator running the published image puts their own name, logo, favicon
//! and colours on it from configuration, with no rebuild. Hlin's own name is
//! the default, not a constant.
//!
//! The logo, the favicon and the stylesheet are files, not URLs. The shell
//! serves them from its own origin, so a brand adds no third party to the
//! page's CSP or to what the page needs to be up. They are checked when the
//! shell starts and read again on every request, so a file replaced on disk (a
//! ConfigMap updated in place) is served without a restart.
//!
//! The stylesheet is how colours change. `hlin-ui` links it after the design
//! pack's own styles, so it may set the chrome's `--hlin-*` roles, the pack's
//! own tokens, or anything else. Modules follow, because the shell sends them
//! the colours the page resolves to. When there is no stylesheet, the route
//! answers with an empty one, so the front end can link it always, without
//! waiting to learn whether there is one.

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::server::AppState;

/// The name the shell wears when nobody configured another.
pub const DEFAULT_NAME: &str = "Hlin";

/// The most a brand file may weigh. A logo, a favicon or a stylesheet that is
/// larger is a mistake, such as a photograph or a bundled font. It is better
/// refused when the shell starts than downloaded by every person on every page.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// The most characters a name may have. It sits in a bar beside the controls.
pub const MAX_NAME_CHARS: usize = 64;

/// Where the front end finds each file.
pub const STYLESHEET_PATH: &str = "/brand/style.css";
/// The logo, for the bar.
pub const LOGO_PATH: &str = "/brand/logo";
/// The favicon, where a browser asks for it unprompted.
pub const FAVICON_PATH: &str = "/favicon.ico";

/// `[brand]` in `hlin.toml`. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrandConfig {
    /// The product's name: in the bar, the page title, and the messages that
    /// name it. Default "Hlin".
    #[serde(default)]
    pub name: Option<String>,

    /// An image for the bar, beside the name: svg, png, jpg or webp.
    #[serde(default)]
    pub logo: Option<PathBuf>,

    /// The page's icon: ico, png or svg.
    #[serde(default)]
    pub favicon: Option<PathBuf>,

    /// CSS put on the page after the design pack's.
    #[serde(default)]
    pub stylesheet: Option<PathBuf>,
}

/// One kind of brand file: what it is called in the configuration, and what it
/// may be.
struct Kind {
    setting: &'static str,
    extensions: &'static [(&'static str, &'static str)],
}

const LOGO: Kind = Kind {
    setting: "brand.logo",
    extensions: &[
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("webp", "image/webp"),
    ],
};

const FAVICON: Kind = Kind {
    setting: "brand.favicon",
    extensions: &[
        ("ico", "image/x-icon"),
        ("png", "image/png"),
        ("svg", "image/svg+xml"),
    ],
};

const STYLESHEET: Kind = Kind {
    setting: "brand.stylesheet",
    extensions: &[("css", "text/css; charset=utf-8")],
};

impl BrandConfig {
    /// The name to show.
    pub fn name(&self) -> &str {
        self.name.as_deref().map(str::trim).unwrap_or(DEFAULT_NAME)
    }

    /// Whether there is a logo to draw.
    pub fn has_logo(&self) -> bool {
        self.logo.is_some()
    }

    /// Every rule that can be checked before a request: a usable name, and
    /// each file there, readable, small enough, and of a type it may be.
    pub fn check(&self) -> Result<(), String> {
        if let Some(name) = &self.name {
            let name = name.trim();
            if name.is_empty() {
                return Err("brand.name is empty; leave it out to use the default".to_string());
            }
            if name.chars().count() > MAX_NAME_CHARS {
                return Err(format!(
                    "brand.name is longer than {MAX_NAME_CHARS} characters"
                ));
            }
        }
        for (path, kind) in [
            (&self.logo, &LOGO),
            (&self.favicon, &FAVICON),
            (&self.stylesheet, &STYLESHEET),
        ] {
            if let Some(path) = path {
                check_file(path, kind)?;
            }
        }
        Ok(())
    }
}

/// The content type a file of this kind is served as, by its extension.
fn content_type(path: &Path, kind: &Kind) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    kind.extensions
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, content_type)| *content_type)
}

fn check_file(path: &Path, kind: &Kind) -> Result<(), String> {
    let shown = path.display();
    if content_type(path, kind).is_none() {
        let allowed: Vec<&str> = kind.extensions.iter().map(|(known, _)| *known).collect();
        return Err(format!(
            "{} `{shown}` must end in .{}",
            kind.setting,
            allowed.join(", .")
        ));
    }
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("{} `{shown}` cannot be read: {error}", kind.setting))?;
    if !metadata.is_file() {
        return Err(format!("{} `{shown}` is not a file", kind.setting));
    }
    if metadata.len() > MAX_BYTES {
        return Err(format!(
            "{} `{shown}` is {} bytes; the most a brand file may be is {MAX_BYTES}",
            kind.setting,
            metadata.len()
        ));
    }
    std::fs::File::open(path)
        .map_err(|error| format!("{} `{shown}` cannot be read: {error}", kind.setting))?;
    Ok(())
}

/// `GET /brand/style.css`: the operator's stylesheet, or an empty one.
pub async fn stylesheet(State(state): State<AppState>) -> Response {
    match &state.config.brand.stylesheet {
        Some(path) => serve(path, &STYLESHEET).await,
        None => file_response(STYLESHEET.extensions[0].1, Vec::new()),
    }
}

/// `GET /brand/logo`: the operator's logo, or 404.
pub async fn logo(State(state): State<AppState>) -> Response {
    match &state.config.brand.logo {
        Some(path) => serve(path, &LOGO).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /favicon.ico`: the operator's favicon, or 404.
///
/// Routed even with none configured. Otherwise the frontend's fallback answers
/// a browser's unprompted request with `index.html`, as an icon.
pub async fn favicon(State(state): State<AppState>) -> Response {
    match &state.config.brand.favicon {
        Some(path) => serve(path, &FAVICON).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A file checked at startup, read again now.
///
/// A file that has gone since is a 404 and a warning, not a crash. The page
/// draws without it, and the operator has a log line naming the setting.
async fn serve(path: &Path, kind: &Kind) -> Response {
    let Some(content_type) = content_type(path, kind) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read(path).await {
        Ok(bytes) if bytes.len() as u64 <= MAX_BYTES => file_response(content_type, bytes),
        Ok(bytes) => {
            tracing::warn!(
                setting = kind.setting,
                path = %path.display(),
                bytes = bytes.len(),
                "brand file has grown past the limit since startup; not serving it"
            );
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            tracing::warn!(
                setting = kind.setting,
                path = %path.display(),
                %error,
                "brand file cannot be read; not serving it"
            );
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// A brand file as served.
///
/// `no-cache`, so a browser asks again each time and a replaced file is seen
/// at once; the files are small. `nosniff`, and a CSP that runs nothing: an
/// SVG opened directly is a document on the shell's origin, and an image
/// should not be able to run script there.
fn file_response(content_type: &'static str, bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; img-src data:",
            ),
        ],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hlin-brand-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn with_nothing_configured_the_name_is_hlins() {
        let brand = BrandConfig::default();
        assert_eq!(brand.name(), DEFAULT_NAME);
        assert!(!brand.has_logo());
        brand.check().unwrap();
    }

    #[test]
    fn a_name_is_trimmed_and_must_say_something() {
        let named = |name: &str| BrandConfig {
            name: Some(name.to_string()),
            ..BrandConfig::default()
        };
        assert_eq!(named("  Acme Work ").name(), "Acme Work");
        assert!(named("   ").check().unwrap_err().contains("brand.name"));
        assert!(named(&"x".repeat(MAX_NAME_CHARS + 1)).check().is_err());
        named(&"x".repeat(MAX_NAME_CHARS)).check().unwrap();
    }

    #[test]
    fn each_file_must_be_there_small_and_of_its_kind() {
        let logo = written("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
        let sheet = written("brand.css", b":root { --hlin-accent: #c2185b; }");
        let icon = written("favicon.ico", &[0, 0, 1, 0]);
        let good = BrandConfig {
            name: Some("Acme".into()),
            logo: Some(logo.clone()),
            favicon: Some(icon),
            stylesheet: Some(sheet.clone()),
        };
        good.check().unwrap();
        assert!(good.has_logo());

        let missing = BrandConfig {
            logo: Some("/tmp/hlin-no-such-logo.svg".into()),
            ..BrandConfig::default()
        };
        assert!(missing.check().unwrap_err().contains("brand.logo"));

        // A stylesheet given as a logo, and a logo given as a stylesheet.
        let swapped = BrandConfig {
            logo: Some(sheet),
            ..BrandConfig::default()
        };
        assert!(swapped.check().unwrap_err().contains(".svg"));
        let swapped = BrandConfig {
            stylesheet: Some(logo),
            ..BrandConfig::default()
        };
        assert!(swapped.check().unwrap_err().contains("brand.stylesheet"));

        let heavy = written("heavy.png", &vec![0u8; MAX_BYTES as usize + 1]);
        let heavy = BrandConfig {
            logo: Some(heavy),
            ..BrandConfig::default()
        };
        assert!(heavy.check().unwrap_err().contains("bytes"));
    }

    #[test]
    fn a_type_is_read_off_the_extension_whatever_its_case() {
        assert_eq!(
            content_type(Path::new("/b/LOGO.SVG"), &LOGO),
            Some("image/svg+xml")
        );
        assert_eq!(content_type(Path::new("/b/logo.gif"), &LOGO), None);
        assert_eq!(content_type(Path::new("/b/logo"), &LOGO), None);
    }

    #[test]
    fn unknown_settings_are_refused_rather_than_ignored() {
        let parsed: Result<BrandConfig, _> = toml::from_str("name = \"Acme\"\ncolour = \"red\"");
        assert!(parsed.is_err());
    }
}
