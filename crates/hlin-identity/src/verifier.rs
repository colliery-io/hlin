//! Checking a token, all of it, every time.
//!
//! The API here is deliberately narrow. There is one way to verify, it takes
//! the audience the caller expects, and it performs every check the
//! specification requires. A platform cannot check a signature and forget the
//! audience, because there is no function that does only the first part.
//!
//! That single failure is worth designing around: a token minted for one
//! platform, replayed against another, is accepted by every implementation
//! that checks only the signature, and nothing in ordinary testing reveals it.

use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use thiserror::Error;

use crate::binding::{RequestRefusal, is_read, normalise_path};
use crate::claims::Claims;
use crate::keys::{Jwks, JwksFetcher};
use crate::{BOUND_TOKEN_LIFETIME_SECONDS, CLOCK_SKEW_SECONDS};

/// How long a fetched key set is trusted before refetching.
const CACHE_FOR: Duration = Duration::from_secs(3600);

/// The shortest gap between two refetches prompted by an unknown key.
///
/// Without this, a flood of tokens signed by a key that genuinely does not
/// exist would become a flood of requests at the shell.
const REFETCH_NO_MORE_THAN_EVERY: Duration = Duration::from_secs(60);

/// Why a token was not accepted.
///
/// A platform maps these to a status code, and the distinction matters: every
/// one of these is a 401, because they all mean the credential is wrong rather
/// than that the principal lacks permission. A 403 is the platform's own
/// decision, made after this succeeds ([[HLIN-S-0004]]).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refusal {
    /// No token was presented.
    #[error("no identity was presented")]
    Absent,
    /// The token is not a JWT, or not one this crate can read.
    #[error("identity is unreadable: {0}")]
    Unreadable(String),
    /// The token names no key, so there is no way to know what signed it.
    #[error("identity names no key")]
    NoKeyId,
    /// The token names a key this platform cannot find, even after refetching.
    #[error("identity names key `{0}`, which the issuer does not publish")]
    UnknownKey(String),
    /// The signature does not match.
    #[error("identity signature is not valid")]
    BadSignature,
    /// The token was minted for a different platform.
    #[error("identity was minted for `{presented}`, not `{expected}`")]
    WrongAudience {
        /// What the token says.
        presented: String,
        /// What this platform is.
        expected: String,
    },
    /// The token was minted by someone else.
    #[error("identity was minted by `{0}`, which is not the configured issuer")]
    WrongIssuer(String),
    /// The token has expired.
    #[error("identity has expired")]
    Expired,
    /// The key set could not be fetched.
    #[error("could not reach the issuer's keys: {0}")]
    KeysUnavailable(String),
}

/// Verifies tokens minted by one shell.
pub struct Verifier {
    issuer: String,
    keys: RwLock<Cached>,
    fetcher: Option<Box<dyn JwksFetcher>>,
    last_refetch: Mutex<Option<Instant>>,
}

struct Cached {
    jwks: Jwks,
    fetched_at: Option<Instant>,
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Verifier")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

impl Verifier {
    /// A verifier holding a fixed key set, with no way to refetch.
    ///
    /// For tests, and for a deployment that distributes keys by configuration.
    pub fn with_keys(issuer: impl Into<String>, jwks: Jwks) -> Self {
        Self {
            issuer: issuer.into(),
            keys: RwLock::new(Cached {
                jwks,
                fetched_at: None,
            }),
            fetcher: None,
            last_refetch: Mutex::new(None),
        }
    }

    /// A verifier that fetches the shell's keys and keeps them for an hour.
    pub fn fetching(issuer: impl Into<String>, fetcher: Box<dyn JwksFetcher>) -> Self {
        Self {
            issuer: issuer.into(),
            keys: RwLock::new(Cached {
                jwks: Jwks::default(),
                fetched_at: None,
            }),
            fetcher: Some(fetcher),
            last_refetch: Mutex::new(None),
        }
    }

    /// Check a token, and say who it is for.
    ///
    /// `expected_audience` is this platform's own id. It is a parameter rather
    /// than configuration so that it cannot be left unset, and every other
    /// check the specification requires happens here too: signature, issuer,
    /// expiry, and clock skew.
    pub fn verify(&self, token: &str, expected_audience: &str) -> Result<Claims, Refusal> {
        if token.trim().is_empty() {
            return Err(Refusal::Absent);
        }

        let header = jsonwebtoken::decode_header(token)
            .map_err(|error| Refusal::Unreadable(error.to_string()))?;
        let kid = header.kid.ok_or(Refusal::NoKeyId)?;

        let key = match self.key_bytes(&kid)? {
            Some(bytes) => bytes,
            None => {
                // A key we have never seen is the ordinary consequence of a
                // rotation, so refetch once before refusing.
                self.refetch_if_allowed()?;
                self.key_bytes(&kid)?
                    .ok_or_else(|| Refusal::UnknownKey(kid.clone()))?
            }
        };

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&[expected_audience]);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.leeway = CLOCK_SKEW_SECONDS;
        validation.validate_exp = true;

        let decoded =
            jsonwebtoken::decode::<Claims>(token, &DecodingKey::from_ed_der(&key), &validation)
                .map_err(|error| translate(error, expected_audience))?;

        Ok(decoded.claims)
    }

    /// Check a token for this request, and say who it is for.
    ///
    /// Everything [`Verifier::verify`] checks, and then the binding. `method`
    /// is the request's method exactly as received, and `path` its path
    /// relative to this platform's base; a query on the end is ignored.
    ///
    /// A `GET` or `HEAD` with an unbound token is accepted exactly as `verify`
    /// would accept it, so reads are unchanged. Any other method needs a token
    /// bound to it: `htm` equal to the method, `htu` equal to the path once
    /// both are in the normal form of [`crate::normalise_path`], and a lifetime
    /// no longer than a bound token's. A token that is bound is held to its
    /// binding whatever the method, since it was minted for one request only.
    pub fn verify_request(
        &self,
        token: &str,
        expected_audience: &str,
        method: &str,
        path: &str,
    ) -> Result<Claims, RequestRefusal> {
        let claims = self.verify(token, expected_audience)?;

        if !claims.is_bound() {
            return if is_read(method) {
                Ok(claims)
            } else {
                Err(RequestRefusal::Unbound(method.to_string()))
            };
        }

        let (Some(bound_method), Some(bound_path)) = (claims.htm(), claims.htu()) else {
            return Err(RequestRefusal::MalformedBinding);
        };

        // A shell that minted a long-lived bound token has a bug, and the
        // thirty seconds are the part of the binding that limits replay of the
        // same request. Checked on the claims rather than trusted.
        if claims.exp - claims.iat > BOUND_TOKEN_LIFETIME_SECONDS {
            return Err(RequestRefusal::TooLong);
        }

        if bound_method != method {
            return Err(RequestRefusal::WrongMethod {
                bound: bound_method.to_string(),
                presented: method.to_string(),
            });
        }

        let presented = normalise_path(path).map_err(RequestRefusal::UnsafeRequest)?;
        if bound_path != presented {
            return Err(RequestRefusal::WrongPath {
                bound: bound_path.to_string(),
                presented,
            });
        }

        Ok(claims)
    }

    fn key_bytes(&self, kid: &str) -> Result<Option<Vec<u8>>, Refusal> {
        let cached = self.keys.read().expect("key cache is not poisoned");

        let stale = match cached.fetched_at {
            Some(at) => at.elapsed() > CACHE_FOR,
            None => self.fetcher.is_some() && cached.jwks.keys.is_empty(),
        };
        if stale {
            drop(cached);
            self.refetch_if_allowed()?;
            let refreshed = self.keys.read().expect("key cache is not poisoned");
            return Ok(refreshed.jwks.find(kid).and_then(|key| key.bytes()));
        }

        Ok(cached.jwks.find(kid).and_then(|key| key.bytes()))
    }

    /// Fetch the key set, unless another caller just did.
    ///
    /// The write lock is held across the fetch on purpose. A platform coming up
    /// under load meets a burst of requests at once, and every one of them has
    /// a cold cache; without serialising here, the first would fetch while the
    /// rest were turned away by the rate limit and refused perfectly good
    /// tokens. Holding the lock makes the others wait for the answer the first
    /// one is already getting.
    ///
    /// Fetches are rare — once an hour, or on a key nobody has seen — so the
    /// contention this creates costs nothing worth measuring.
    fn refetch_if_allowed(&self) -> Result<(), Refusal> {
        let Some(fetcher) = &self.fetcher else {
            return Ok(());
        };

        let mut cached = self.keys.write().expect("key cache is not poisoned");

        // Someone may have fetched while this caller waited for the lock.
        if let Some(at) = cached.fetched_at
            && at.elapsed() < CACHE_FOR
        {
            return Ok(());
        }

        {
            let mut last = self
                .last_refetch
                .lock()
                .expect("refetch clock is not poisoned");

            // The rate limit exists so that tokens naming keys that do not
            // exist cannot become a flood of requests at the shell. It applies
            // only once something has been fetched at least once: a cold cache
            // must always be allowed to fill, or a platform that starts before
            // the shell never recovers.
            let cold = cached.fetched_at.is_none();
            if !cold
                && let Some(at) = *last
                && at.elapsed() < REFETCH_NO_MORE_THAN_EVERY
            {
                return Ok(());
            }
            *last = Some(Instant::now());
        }

        let fetched = fetcher.fetch().map_err(Refusal::KeysUnavailable)?;
        cached.jwks = fetched;
        cached.fetched_at = Some(Instant::now());
        Ok(())
    }
}

fn translate(error: jsonwebtoken::errors::Error, expected: &str) -> Refusal {
    use jsonwebtoken::errors::ErrorKind;

    match error.kind() {
        ErrorKind::InvalidSignature => Refusal::BadSignature,
        ErrorKind::ExpiredSignature => Refusal::Expired,
        ErrorKind::InvalidAudience => Refusal::WrongAudience {
            // The library does not hand back what it saw, and reading the
            // token again to find out would mean trusting an unverified
            // payload to write a log line.
            presented: "another platform".to_string(),
            expected: expected.to_string(),
        },
        ErrorKind::InvalidIssuer => Refusal::WrongIssuer("another issuer".to_string()),
        _ => Refusal::Unreadable(error.to_string()),
    }
}
