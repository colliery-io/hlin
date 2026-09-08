//! Resolving the paths a manifest declares against the platform's base.
//!
//! Platforms live behind path-prefixed routing, so ordinary URL resolution is
//! the wrong rule: joining `/api/hlin/x` to `https://host/orebank/` by the usual
//! scheme would escape the prefix and land on the shell's own root. Instead a
//! manifest path is *appended* to the base after any leading slash is stripped,
//! and anything that could point outside the base is malformed (REQ-1.2).

use std::fmt;

/// Why a declared path cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDefect {
    /// The path was empty.
    Empty,
    /// The path carried a scheme, so it names some other origin.
    AbsoluteUrl,
    /// The path began `//`, which resolves against the current scheme's
    /// authority and so names some other host.
    SchemeRelative,
    /// The path walked upwards, so it could leave the platform's prefix.
    ParentSegment,
}

impl fmt::Display for PathDefect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Empty => "path is empty",
            Self::AbsoluteUrl => {
                "path is an absolute URL; manifests declare paths relative to the platform base"
            }
            Self::SchemeRelative => {
                "path is scheme-relative; manifests declare paths relative to the platform base"
            }
            Self::ParentSegment => "path contains a `..` segment",
        };
        formatter.write_str(reason)
    }
}

/// Check that a declared path is usable, returning it in the normalised form
/// that [`resolve`] appends to a base.
///
/// A single leading `/` is tolerated and stripped, because writing `/api/x` is
/// the natural instinct and means the same thing here.
pub fn normalize(path: &str) -> Result<&str, PathDefect> {
    if path.is_empty() {
        return Err(PathDefect::Empty);
    }
    if path.starts_with("//") {
        return Err(PathDefect::SchemeRelative);
    }
    if has_scheme(path) {
        return Err(PathDefect::AbsoluteUrl);
    }

    let trimmed = path.strip_prefix('/').unwrap_or(path);
    if trimmed.is_empty() {
        return Err(PathDefect::Empty);
    }
    if trimmed.split('/').any(|segment| segment == "..") {
        return Err(PathDefect::ParentSegment);
    }

    Ok(trimmed)
}

/// Resolve a declared path against the platform base the manifest was fetched
/// from.
pub fn resolve(base: &str, path: &str) -> Result<String, PathDefect> {
    let normalized = normalize(path)?;
    let base = base.trim_end_matches('/');
    Ok(format!("{base}/{normalized}"))
}

/// Whether a path starts with something that parses as a URL scheme.
///
/// Follows RFC 3986: a scheme is a letter followed by letters, digits, `+`, `-`
/// or `.`, terminated by a colon.
fn has_scheme(path: &str) -> bool {
    let mut characters = path.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for character in characters {
        match character {
            ':' => return true,
            c if c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.' => {}
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_slash_is_stripped_rather_than_rejected() {
        assert_eq!(normalize("/api/hlin/x"), Ok("api/hlin/x"));
        assert_eq!(normalize("api/hlin/x"), Ok("api/hlin/x"));
    }

    #[test]
    fn paths_stay_inside_the_platform_prefix() {
        assert_eq!(
            resolve("https://host/orebank", "/api/hlin/x").as_deref(),
            Ok("https://host/orebank/api/hlin/x")
        );
        assert_eq!(
            resolve("https://host/orebank/", "api/hlin/x").as_deref(),
            Ok("https://host/orebank/api/hlin/x")
        );
    }

    #[test]
    fn other_origins_are_rejected() {
        assert_eq!(
            normalize("https://elsewhere/x"),
            Err(PathDefect::AbsoluteUrl)
        );
        assert_eq!(
            normalize("HTTP://elsewhere/x"),
            Err(PathDefect::AbsoluteUrl)
        );
        assert_eq!(normalize("file:/etc/passwd"), Err(PathDefect::AbsoluteUrl));
        assert_eq!(normalize("//elsewhere/x"), Err(PathDefect::SchemeRelative));
    }

    #[test]
    fn upward_walks_are_rejected() {
        assert_eq!(normalize("../other/x"), Err(PathDefect::ParentSegment));
        assert_eq!(normalize("api/../../x"), Err(PathDefect::ParentSegment));
        assert_eq!(normalize("/api/.."), Err(PathDefect::ParentSegment));
    }

    #[test]
    fn dots_inside_a_segment_are_ordinary() {
        assert_eq!(normalize("api/v1.2/x"), Ok("api/v1.2/x"));
        assert_eq!(normalize("api/..x"), Ok("api/..x"));
    }

    #[test]
    fn empty_paths_are_rejected() {
        assert_eq!(normalize(""), Err(PathDefect::Empty));
        assert_eq!(normalize("/"), Err(PathDefect::Empty));
    }
}
