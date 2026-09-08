//! Minting, and where a shell's key lives.

use std::path::Path;

use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use thiserror::Error;

use crate::TOKEN_LIFETIME_SECONDS;
use crate::claims::{Claims, Principal};
use crate::keys::{Jwks, PublicKey, base64url, from_base64url, pkcs8_from_seed};

/// Why a token could not be minted, or a key could not be loaded.
#[derive(Debug, Error)]
pub enum IssuerError {
    /// The key file could not be read or written.
    #[error("key file: {0}")]
    KeyFile(String),
    /// The key file held something that is not a key.
    #[error("key file does not contain a 32-byte Ed25519 seed")]
    MalformedKey,
    /// Signing failed.
    #[error("could not sign: {0}")]
    Signing(String),
}

/// The shell's signing key, and the identity it signs as.
pub struct Issuer {
    issuer: String,
    kid: String,
    seed: [u8; 32],
    public: [u8; 32],
    encoding: EncodingKey,
}

impl std::fmt::Debug for Issuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the seed.
        formatter
            .debug_struct("Issuer")
            .field("issuer", &self.issuer)
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}

impl Issuer {
    /// A new random key.
    pub fn generate(issuer: impl Into<String>) -> Self {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the system random source is available");
        Self::from_seed(issuer, seed)
    }

    /// An issuer from a known seed.
    pub fn from_seed(issuer: impl Into<String>, seed: [u8; 32]) -> Self {
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let public = signing.verifying_key().to_bytes();

        // The identifier is derived from the public key rather than chosen, so
        // two shells cannot pick the same one and a key's identifier always
        // matches the key.
        let kid = base64url(&public)[..16].to_string();

        Self {
            issuer: issuer.into(),
            kid,
            seed,
            public,
            encoding: EncodingKey::from_ed_der(&pkcs8_from_seed(&seed)),
        }
    }

    /// Load the key at this path, or generate and save one.
    ///
    /// A demo shell that regenerated its key on every restart would change its
    /// `kid` each time and force every platform to refetch, which is noise
    /// rather than a demonstration of rotation.
    pub fn load_or_generate(issuer: impl Into<String>, path: &Path) -> Result<Self, IssuerError> {
        let issuer = issuer.into();

        if path.exists() {
            let text = std::fs::read_to_string(path)
                .map_err(|error| IssuerError::KeyFile(error.to_string()))?;
            let bytes = from_base64url(text.trim()).ok_or(IssuerError::MalformedKey)?;
            let seed: [u8; 32] = bytes.try_into().map_err(|_| IssuerError::MalformedKey)?;
            return Ok(Self::from_seed(issuer, seed));
        }

        let created = Self::generate(issuer);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| IssuerError::KeyFile(error.to_string()))?;
        }
        std::fs::write(path, base64url(&created.seed))
            .map_err(|error| IssuerError::KeyFile(error.to_string()))?;
        Ok(created)
    }

    /// This issuer's name, as it appears in `iss`.
    pub fn name(&self) -> &str {
        &self.issuer
    }

    /// This key's identifier.
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// The key set a platform verifies against.
    pub fn jwks(&self) -> Jwks {
        Jwks {
            keys: vec![PublicKey {
                kty: "OKP".to_string(),
                crv: "Ed25519".to_string(),
                kid: self.kid.clone(),
                x: base64url(&self.public),
                r#use: "sig".to_string(),
                alg: "EdDSA".to_string(),
            }],
        }
    }

    /// Mint a token for this principal, for this platform and no other.
    ///
    /// The audience is required rather than optional because a token without
    /// one is a token that works everywhere, which is the failure this whole
    /// crate exists to prevent.
    pub fn mint(&self, principal: &Principal, audience: &str) -> Result<String, IssuerError> {
        let now = Utc::now().timestamp();

        let claims = Claims {
            iss: self.issuer.clone(),
            sub: principal.sub.clone(),
            aud: audience.to_string(),
            iat: now,
            exp: now + TOKEN_LIFETIME_SECONDS,
            jti: uuid::Uuid::new_v4().to_string(),
            name: principal.name.clone(),
            email: principal.email.clone(),
            groups: principal.groups.clone(),
            extra: Default::default(),
        };

        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());

        jsonwebtoken::encode(&header, &claims, &self.encoding)
            .map_err(|error| IssuerError::Signing(error.to_string()))
    }
}
