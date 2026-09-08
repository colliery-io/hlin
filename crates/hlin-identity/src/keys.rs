//! Keys, and how a platform gets them.

use std::collections::BTreeMap;

use base64::Engine;
use serde::{Deserialize, Serialize};

/// RFC 8410 PKCS#8 v1 wrapper for an Ed25519 seed.
///
/// A fixed 16-byte prefix followed by the 32-byte seed. The JWT library takes
/// its signing key in this container, and the container is a standard with one
/// possible encoding, so building it is arithmetic rather than cryptography.
pub(crate) fn pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(seed);
    der
}

pub(crate) fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn from_base64url(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .ok()
}

/// One public key, in the shape a JWKS document uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey {
    /// Key type. Always `OKP` for Ed25519.
    pub kty: String,
    /// Curve. Always `Ed25519`.
    pub crv: String,
    /// Which key this is. Required, because it is what lets a platform meet a
    /// token signed by a key it has not seen and fetch once rather than fail.
    pub kid: String,
    /// The public key, base64url without padding.
    pub x: String,
    /// Always `sig`.
    #[serde(default = "signature_use")]
    pub r#use: String,
    /// Always `EdDSA`.
    #[serde(default = "eddsa")]
    pub alg: String,
}

fn signature_use() -> String {
    "sig".to_string()
}

fn eddsa() -> String {
    "EdDSA".to_string()
}

impl PublicKey {
    /// The raw 32 bytes, if this key is readable.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        (self.kty == "OKP" && self.crv == "Ed25519")
            .then(|| from_base64url(&self.x))
            .flatten()
    }
}

/// The document a shell publishes so platforms can verify its tokens.
///
/// More than one key is present during a rotation: the new key is published
/// before anything signs with it, so a platform always has a key before it
/// meets a token that needs it (HLIN-S-0004).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Jwks {
    /// The keys, newest first.
    pub keys: Vec<PublicKey>,
}

impl Jwks {
    /// The key with this identifier, if it is published.
    pub fn find(&self, kid: &str) -> Option<&PublicKey> {
        self.keys.iter().find(|key| key.kid == kid)
    }

    /// Every key, by identifier.
    pub fn by_kid(&self) -> BTreeMap<&str, &PublicKey> {
        self.keys
            .iter()
            .map(|key| (key.kid.as_str(), key))
            .collect()
    }
}

/// How a verifier fetches a shell's keys.
///
/// A trait rather than a concrete client so a platform can bring its own HTTP
/// stack, and so tests can serve a key set without a network.
pub trait JwksFetcher: Send + Sync {
    /// Fetch the current key set.
    fn fetch(&self) -> Result<Jwks, String>;
}
