//! Reading a manifest off the wire.
//!
//! Parsing is the only step here that can fail outright: a document that is not
//! JSON, or whose required fields are absent or the wrong type, has nothing for
//! validation to examine. Everything a document says that is merely *wrong* is
//! a classified outcome instead, in [`crate::validate`], because a manifest
//! problem is never a shell error.

use thiserror::Error;

use crate::manifest::Manifest;

/// Why a manifest could not be read at all.
#[derive(Debug, Error)]
pub enum ParseError {
    /// The bytes were not JSON, or the JSON was not shaped like a manifest.
    #[error("manifest is not readable: {0}")]
    Malformed(#[from] serde_json::Error),
}

/// Result alias for reading a manifest.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Read a manifest from the bytes a platform served.
pub fn parse(bytes: &[u8]) -> Result<Manifest> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Read a manifest from a string.
pub fn parse_str(document: &str) -> Result<Manifest> {
    Ok(serde_json::from_str(document)?)
}
