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

/// Why a declared prefix, or a path that must fall under one, cannot be used.
///
/// Prefixes are held to a stricter rule than the paths above, because the shell
/// does more with them. A data path is only ever appended to a base. A prefix
/// is *matched*: the shell serves a module's assets and forwards its requests
/// only when the path asked for falls under a declared prefix, segment by
/// segment (specification HLIN-S-0007). Anything that could make two readers
/// disagree about whether a path falls under a prefix — an encoded slash, a
/// backslash some servers treat as one, an empty segment some collapse — is a
/// way to reach something the platform did not declare, so it is refused here,
/// once, rather than hoped about at every match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixDefect {
    /// Nothing was declared.
    Empty,
    /// It did not begin with `/`. Prefixes are written as they will be matched,
    /// from the platform base, so there is one spelling of each.
    NotRooted,
    /// A prefix did not end with `/`. `/ui` would read as though it matched
    /// `/uix`; `/ui/` cannot be misread.
    NotSegmentShaped,
    /// A prefix named no segment at all. `/` would hand a module every path the
    /// platform serves, including its manifest and its health endpoint.
    WholePlatform,
    /// A path that must name a file ended with `/`.
    NotAFile,
    /// Two slashes in a row. The empty segment between them is collapsed by
    /// some servers and not by others, and a leading `//` names another host.
    EmptySegment,
    /// A `.` or `..` segment, which walks rather than names.
    DotSegment,
    /// A backslash, which some servers treat as a separator.
    Backslash,
    /// A percent-encoded `/`, `\` or `.`, which decodes into one of the above
    /// after the prefix has been checked.
    EncodedSeparator,
    /// A `?` or `#`: a prefix names paths, not queries or fragments.
    QueryOrFragment,
    /// A `:` in the first segment, which a URL parser would read as a scheme.
    Scheme,
}

impl fmt::Display for PrefixDefect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Empty => "path is empty",
            Self::NotRooted => "path must begin with `/`",
            Self::NotSegmentShaped => "prefix must end with `/`",
            Self::WholePlatform => "prefix must name at least one segment",
            Self::NotAFile => "path must name a file, not end with `/`",
            Self::EmptySegment => "path contains an empty segment",
            Self::DotSegment => "path contains a `.` or `..` segment",
            Self::Backslash => "path contains a backslash",
            Self::EncodedSeparator => "path contains a percent-encoded `/`, `\\` or `.`",
            Self::QueryOrFragment => "path contains a `?` or `#`",
            Self::Scheme => "path names a scheme",
        };
        formatter.write_str(reason)
    }
}

/// Check that a declared prefix is usable: `/ui/`, `/api/v1/`.
///
/// A prefix begins and ends with `/`, names at least one segment, and carries
/// nothing that could decode or normalise into a different path.
pub fn check_prefix(prefix: &str) -> Result<(), PrefixDefect> {
    let inner = check_rooted(prefix)?;
    if inner.is_empty() {
        return Err(PrefixDefect::WholePlatform);
    }
    let Some(inner) = inner.strip_suffix('/') else {
        return Err(PrefixDefect::NotSegmentShaped);
    };
    check_segments(inner)
}

/// Check that a path naming one file under a prefix is usable:
/// `/ui/items/index.html`.
///
/// The same rules as [`check_prefix`], except that the path names a file, so
/// it must not end with `/`.
pub fn check_file(path: &str) -> Result<(), PrefixDefect> {
    let inner = check_rooted(path)?;
    if inner.is_empty() || inner.ends_with('/') {
        return Err(PrefixDefect::NotAFile);
    }
    check_segments(inner)
}

/// Whether `path` falls under `prefix`, by segment.
///
/// `/ui/` covers `/ui/items/app.wasm` and `/ui/` itself, never `/uix/app.wasm`.
/// Both arguments are compared as written: checking them is [`check_prefix`]'s
/// job, and a path that fails it falls under nothing.
pub fn falls_under(prefix: &str, path: &str) -> bool {
    check_prefix(prefix).is_ok() && path.starts_with(prefix)
}

fn check_rooted(path: &str) -> Result<&str, PrefixDefect> {
    if path.is_empty() {
        return Err(PrefixDefect::Empty);
    }
    path.strip_prefix('/').ok_or(PrefixDefect::NotRooted)
}

/// Check the part of a rooted path after its leading `/`.
fn check_segments(inner: &str) -> Result<(), PrefixDefect> {
    if inner.contains('\\') {
        return Err(PrefixDefect::Backslash);
    }
    if inner.contains(['?', '#']) {
        return Err(PrefixDefect::QueryOrFragment);
    }
    let lowered = inner.to_ascii_lowercase();
    if ["%2f", "%5c", "%2e"]
        .iter()
        .any(|encoded| lowered.contains(encoded))
    {
        return Err(PrefixDefect::EncodedSeparator);
    }
    if inner
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'))
    {
        return Err(PrefixDefect::Scheme);
    }
    for segment in inner.split('/') {
        match segment {
            "" => return Err(PrefixDefect::EmptySegment),
            "." | ".." => return Err(PrefixDefect::DotSegment),
            _ => {}
        }
    }
    Ok(())
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

    #[test]
    fn a_prefix_is_rooted_and_segment_shaped() {
        assert_eq!(check_prefix("/ui/"), Ok(()));
        assert_eq!(check_prefix("/api/v1.2/"), Ok(()));
        assert_eq!(check_prefix(""), Err(PrefixDefect::Empty));
        assert_eq!(check_prefix("ui/"), Err(PrefixDefect::NotRooted));
        assert_eq!(check_prefix("/ui"), Err(PrefixDefect::NotSegmentShaped));
        assert_eq!(check_prefix("/"), Err(PrefixDefect::WholePlatform));
    }

    #[test]
    fn a_prefix_cannot_name_another_origin() {
        assert_eq!(
            check_prefix("https://elsewhere/"),
            Err(PrefixDefect::NotRooted)
        );
        assert_eq!(
            check_prefix("//elsewhere/ui/"),
            Err(PrefixDefect::EmptySegment)
        );
        assert_eq!(check_prefix("/https:/ui/"), Err(PrefixDefect::Scheme));
    }

    #[test]
    fn a_prefix_carries_nothing_that_decodes_into_another_path() {
        assert_eq!(check_prefix("/ui/../api/"), Err(PrefixDefect::DotSegment));
        assert_eq!(check_prefix("/ui/./"), Err(PrefixDefect::DotSegment));
        assert_eq!(check_prefix("/ui//x/"), Err(PrefixDefect::EmptySegment));
        assert_eq!(check_prefix("/ui\\x/"), Err(PrefixDefect::Backslash));
        assert_eq!(
            check_prefix("/ui%2Fx/"),
            Err(PrefixDefect::EncodedSeparator)
        );
        assert_eq!(
            check_prefix("/ui%5cx/"),
            Err(PrefixDefect::EncodedSeparator)
        );
        assert_eq!(
            check_prefix("/%2e%2e/"),
            Err(PrefixDefect::EncodedSeparator)
        );
        assert_eq!(check_prefix("/ui?x=1/"), Err(PrefixDefect::QueryOrFragment));
        assert_eq!(check_prefix("/ui#x/"), Err(PrefixDefect::QueryOrFragment));
    }

    #[test]
    fn a_file_path_names_a_file() {
        assert_eq!(check_file("/ui/items/index.html"), Ok(()));
        assert_eq!(check_file("/ui/items/"), Err(PrefixDefect::NotAFile));
        assert_eq!(check_file("/"), Err(PrefixDefect::NotAFile));
        assert_eq!(check_file("ui/index.html"), Err(PrefixDefect::NotRooted));
        assert_eq!(
            check_file("/ui/../index.html"),
            Err(PrefixDefect::DotSegment)
        );
    }

    #[test]
    fn a_prefix_covers_by_segment_not_by_string() {
        assert!(falls_under("/ui/", "/ui/items/app.wasm"));
        assert!(falls_under("/ui/", "/ui/"));
        assert!(!falls_under("/ui/", "/uix/app.wasm"));
        assert!(!falls_under("/ui/", "/ui"));
        assert!(
            !falls_under("/ui", "/uix/app.wasm"),
            "an unusable prefix covers nothing"
        );
    }
}
