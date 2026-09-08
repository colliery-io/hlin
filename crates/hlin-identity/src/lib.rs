//! Who is asking, and how a platform knows.
//!
//! Hlin authenticates a person once and then calls platforms on their behalf
//! (decision HLIN-A-0004). This crate is the credential that carries who they
//! are: the shell mints a short-lived signed token per request, publishes the
//! key that verifies it, and a platform checks it without ever calling the
//! shell back. Specification HLIN-S-0004 defines the token; specification
//! HLIN-S-0005 places it as the `hlin-token` strategy among several.
//!
//! Every platform depends on this crate to answer one question, so its API is
//! shaped around the failure that matters. A platform that verifies a
//! signature but forgets the audience will happily accept a token minted for
//! somebody else, and nothing about that failure is visible in testing. There
//! is therefore no way to check a signature without also checking the
//! audience: [`Verifier::verify`] takes the expected audience and does all of
//! it, or refuses.
//!
//! ```
//! use hlin_identity::{Issuer, Principal, Verifier};
//!
//! let issuer = Issuer::generate("hlin");
//! let principal = Principal::new("u_01H8XK2P");
//! let token = issuer.mint(&principal, "orebank").expect("mints");
//!
//! // A platform verifies against its own id, with the shell's public keys.
//! let verifier = Verifier::with_keys("hlin", issuer.jwks());
//! assert!(verifier.verify(&token, "orebank").is_ok());
//!
//! // The same token is worthless against a different platform.
//! assert!(verifier.verify(&token, "stampmill").is_err());
//! ```

#![warn(missing_docs)]

// The development bypass exists so a platform team can run their service with
// no shell in front of it. A flag that can be set at runtime is a flag that
// will be set, at three in the morning, to make something work; so this one
// cannot survive a release build.
#[cfg(all(feature = "dev-identity", not(debug_assertions)))]
compile_error!(
    "the `dev-identity` feature accepts unsigned requests and must never be \
     compiled into a release build; remove it from your feature list"
);

mod claims;
mod issuer;
mod keys;
mod verifier;

#[cfg(feature = "axum")]
pub mod extract;

pub use claims::{Claims, Principal};
pub use issuer::{Issuer, IssuerError};
pub use keys::{Jwks, JwksFetcher, PublicKey};
pub use verifier::{Refusal, Verifier};

/// The header carrying the identity the shell forwards.
///
/// Deliberately not `Authorization`: a platform's own authorization semantics
/// are its business, and Hlin must not collide with them.
pub const IDENTITY_HEADER: &str = "X-Hlin-Identity";

/// Where a shell publishes the keys that verify its tokens.
pub const JWKS_PATH: &str = ".well-known/hlin-keys.json";

/// How long a minted token is good for.
///
/// A token is minted per request and needs only to outlive that request, so
/// this is short on purpose. A long-lived browser stream will outlive any sane
/// token lifetime, which is exactly why tokens are per request rather than per
/// stream (HLIN-S-0004 REQ-1.3).
pub const TOKEN_LIFETIME_SECONDS: i64 = 120;

/// How much clock difference between shell and platform is tolerated.
pub const CLOCK_SKEW_SECONDS: u64 = 60;
