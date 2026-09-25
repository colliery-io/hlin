//! Tying a write's token to the one request it was minted for.
//!
//! A read token captured from a log lets whoever holds it read what that
//! person could read, for two minutes. The same token accepted on a write
//! would let them act as that person instead, which is a different kind of
//! harm. So a write carries a token that names its own method (`htm`) and path
//! (`htu`), lives for thirty seconds, and is good for that request and no
//! other ([[HLIN-A-0013]] decision 4). Reads stay unbound.
//!
//! The hard part is the path. The shell binds the path it sends and the
//! platform checks the path it receives, and the two must agree on what "the
//! same path" means, or a look-alike slips through: `/api/%61ctions` for
//! `/api/actions`, `/api/actions/` for `/api/actions`, `/api//actions`,
//! `/api/x/../actions`. Both sides therefore reduce a path to one normal form
//! with [`normalise_path`] before comparing, and anything that form cannot
//! represent unambiguously is refused rather than guessed at.

use thiserror::Error;

use crate::verifier::Refusal;

/// The claim naming the method a token is bound to.
pub const METHOD_CLAIM: &str = "htm";

/// The claim naming the path a token is bound to.
pub const PATH_CLAIM: &str = "htu";

/// Whether a method only reads.
///
/// Exactly `GET` and `HEAD`, compared as written. Methods are case-sensitive
/// in HTTP, and treating `get` as a read would let a lowercase method slip a
/// write past the binding on a server that is lenient about case. Everything
/// else, including methods nobody has heard of, is a write and must be bound.
pub fn is_read(method: &str) -> bool {
    method == "GET" || method == "HEAD"
}

/// Why a method or path cannot be bound.
///
/// Every one of these is a path that could be read two ways. Refusing is the
/// only answer that cannot be wrong: the shell's own proxy already refuses
/// the same shapes before it mints anything ([[HLIN-S-0007]]), so a request
/// that reaches a platform through the shell never meets these.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum UnsafeRequest {
    /// The method is empty or not a valid HTTP method token.
    #[error("`{0}` is not an HTTP method")]
    Method(String),
    /// The path does not start with `/`.
    #[error("the path is not relative to the platform's base")]
    NotAbsolute,
    /// Two slashes in a row, or a slash at the end.
    #[error("the path has an empty segment")]
    EmptySegment,
    /// A `.` or `..` segment, however it was spelled.
    #[error("the path has a dot segment")]
    DotSegment,
    /// A `%` not followed by two hex digits.
    #[error("the path has a malformed percent escape")]
    BadEscape,
    /// A segment that decodes to a slash, a backslash or a control character,
    /// so that decoding would change where the segments fall.
    #[error("the path encodes a separator or control character")]
    EncodedSeparator,
    /// A literal backslash, which some servers treat as a slash.
    #[error("the path contains a backslash")]
    Backslash,
    /// Bytes that are not UTF-8 once decoded.
    #[error("the path is not UTF-8 once decoded")]
    NotUtf8,
}

/// Reduce a path to the form a binding compares.
///
/// The path is relative to the platform's base and starts with `/`. Anything
/// from the first `?` or `#` is the query or fragment and is not part of it.
/// Each segment is percent-decoded exactly once, so `%41` and `A` are the same
/// path, while `%2541` stays `%41` and is a different one. Case is kept: `/Api`
/// and `/api` are different paths, as they are to almost every router.
///
/// Refused, rather than normalised, are the shapes where two parties could
/// honestly disagree about what was meant: an empty segment (a double slash,
/// or a trailing slash other than the root `/` itself), a `.` or `..` segment
/// in any spelling, a backslash, a malformed escape, and an escape that
/// decodes to a separator or control character. Resolving `..` or collapsing
/// slashes here would make the binding agree with one server's reading of
/// the path and not another's.
pub fn normalise_path(path: &str) -> Result<String, UnsafeRequest> {
    let path = match path.find(['?', '#']) {
        Some(end) => &path[..end],
        None => path,
    };

    if path == "/" {
        return Ok(path.to_string());
    }

    let Some(rest) = path.strip_prefix('/') else {
        return Err(UnsafeRequest::NotAbsolute);
    };

    let mut normal = String::with_capacity(path.len());
    for segment in rest.split('/') {
        if segment.is_empty() {
            return Err(UnsafeRequest::EmptySegment);
        }
        if segment.contains('\\') {
            return Err(UnsafeRequest::Backslash);
        }

        let decoded = decode_segment(segment)?;
        if decoded == "." || decoded == ".." {
            return Err(UnsafeRequest::DotSegment);
        }
        if decoded
            .chars()
            .any(|character| character == '/' || character == '\\' || character.is_control())
        {
            return Err(UnsafeRequest::EncodedSeparator);
        }

        normal.push('/');
        normal.push_str(&decoded);
    }

    Ok(normal)
}

/// One segment, percent-decoded once.
fn decode_segment(segment: &str) -> Result<String, UnsafeRequest> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex(*byte));
            let low = bytes.get(index + 2).and_then(|byte| hex(*byte));
            match (high, low) {
                (Some(high), Some(low)) => decoded.push(high << 4 | low),
                _ => return Err(UnsafeRequest::BadEscape),
            }
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| UnsafeRequest::NotUtf8)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A method is a token in the HTTP grammar: at least one of a fixed set of
/// visible characters, and nothing else.
fn valid_method(method: &str) -> bool {
    !method.is_empty()
        && method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// The request a token is being bound to, already in normal form.
///
/// Built before minting so that a path which cannot be bound is refused
/// before anything is signed, and so the claims always hold the normal form a
/// platform will compare against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRequest {
    method: String,
    path: String,
}

impl BoundRequest {
    /// Bind to this method and path.
    ///
    /// `path` is the path as it will be sent, relative to the platform's base;
    /// any query is dropped. The method is kept exactly as given, since that
    /// is what goes on the wire and what the platform will compare.
    pub fn new(method: &str, path: &str) -> Result<Self, UnsafeRequest> {
        if !valid_method(method) {
            return Err(UnsafeRequest::Method(method.to_string()));
        }
        Ok(Self {
            method: method.to_string(),
            path: normalise_path(path)?,
        })
    }

    /// The method, as it goes in `htm`.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The normalised path, as it goes in `htu`.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Why a token was not accepted for this particular request.
///
/// Every one of these is a 401, for the same reason every [`Refusal`] is: the
/// credential is wrong for the request, which says nothing about whether the
/// principal may do the thing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RequestRefusal {
    /// The token itself was refused, before its binding was looked at.
    #[error(transparent)]
    Identity(#[from] Refusal),
    /// A write presented a token that is not bound to any request.
    #[error("a `{0}` needs identity bound to the request, and this identity is not")]
    Unbound(String),
    /// The binding claims are present but not both strings.
    #[error("identity carries a malformed request binding")]
    MalformedBinding,
    /// The token was bound to a different method.
    #[error("identity was bound to `{bound}`, not `{presented}`")]
    WrongMethod {
        /// What the token says.
        bound: String,
        /// What the request is.
        presented: String,
    },
    /// The token was bound to a different path.
    #[error("identity was bound to `{bound}`, not `{presented}`")]
    WrongPath {
        /// What the token says.
        bound: String,
        /// What the request's path normalises to.
        presented: String,
    },
    /// The request's own method or path cannot be compared safely.
    #[error("the request cannot be matched to a binding: {0}")]
    UnsafeRequest(UnsafeRequest),
    /// A bound token that lives longer than a bound token may.
    #[error("identity is bound to a request but outlives the bound lifetime")]
    TooLong,
}
