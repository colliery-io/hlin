//! Reading the identity off a request, for platforms that use axum.
//!
//! A thin adapter over [`Verifier`], kept behind a feature so a platform on a
//! different HTTP stack can use the rest of this crate without taking on axum.

use std::sync::Arc;

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use crate::IDENTITY_HEADER;
use crate::binding::RequestRefusal;
use crate::claims::Claims;
use crate::verifier::{Refusal, Verifier};

/// What a platform puts in its router state to check identities.
#[derive(Clone)]
pub struct IdentityState {
    /// The verifier for the shell that calls this platform.
    pub verifier: Arc<Verifier>,
    /// This platform's own id, which every token must name as its audience.
    pub audience: String,
    /// Who an unsigned request is, when the development bypass is compiled in.
    #[cfg(feature = "dev-identity")]
    pub development_principal: crate::Principal,
}

/// A verified identity, extracted from a request.
///
/// A handler taking this argument cannot run without a valid identity, which
/// is the point: forgetting the check is not something a platform author can
/// do by omission.
#[derive(Debug, Clone)]
pub struct HlinIdentity(pub Claims);

impl<S> FromRequestParts<S> for HlinIdentity
where
    IdentityState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = IdentityState::from_ref(state);

        let presented = parts
            .headers
            .get(IDENTITY_HEADER)
            .and_then(|value| value.to_str().ok());

        match presented {
            Some(token) => match identity.verifier.verify(token, &identity.audience) {
                Ok(claims) => Ok(HlinIdentity(claims)),
                Err(refusal) => Err(refuse(refusal)),
            },

            #[cfg(feature = "dev-identity")]
            None => {
                tracing::warn!(
                    "accepting an unsigned request as the development principal; \
                     this build must never reach production"
                );
                Ok(HlinIdentity(development_claims(&identity)))
            }

            #[cfg(not(feature = "dev-identity"))]
            None => Err(refuse(Refusal::Absent)),
        }
    }
}

/// A verified identity, checked against the request it arrived on.
///
/// For a platform that accepts writes. On a `GET` or `HEAD` it accepts what
/// [`HlinIdentity`] accepts. On anything else it also requires the token to be
/// bound to this method and this path, so a read token lifted from a log
/// cannot be spent on a write ([[HLIN-A-0013]] decision 4).
///
/// The path compared is the one axum's router sees, which is relative to the
/// platform's base when the platform serves from its root or `nest`s its
/// routes under the base path. A platform behind a proxy that rewrites paths
/// should call [`Verifier::verify_request`] itself with the path the shell
/// addressed.
///
/// A refusal is a 401 whose body says why, because a platform team meeting
/// one for the first time is debugging its own deployment, and "not bound to
/// this request" is the difference between an hour and a minute.
#[derive(Debug, Clone)]
pub struct HlinRequestIdentity(pub Claims);

impl<S> FromRequestParts<S> for HlinRequestIdentity
where
    IdentityState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = IdentityState::from_ref(state);

        let presented = parts
            .headers
            .get(IDENTITY_HEADER)
            .and_then(|value| value.to_str().ok());

        match presented {
            Some(token) => match identity.verifier.verify_request(
                token,
                &identity.audience,
                parts.method.as_str(),
                parts.uri.path(),
            ) {
                Ok(claims) => Ok(HlinRequestIdentity(claims)),
                Err(refusal) => Err(refuse_request(refusal)),
            },

            // The bypass is unbound, writes included: a platform developing
            // with no shell has nothing to bind a token, and the bypass cannot
            // reach a release build to be abused there.
            #[cfg(feature = "dev-identity")]
            None => {
                tracing::warn!(
                    "accepting an unsigned request as the development principal; \
                     this build must never reach production"
                );
                Ok(HlinRequestIdentity(development_claims(&identity)))
            }

            #[cfg(not(feature = "dev-identity"))]
            None => Err(refuse_request(RequestRefusal::Identity(Refusal::Absent))),
        }
    }
}

#[cfg(feature = "dev-identity")]
fn development_claims(identity: &IdentityState) -> Claims {
    Claims {
        iss: "development".to_string(),
        sub: identity.development_principal.sub.clone(),
        aud: identity.audience.clone(),
        iat: 0,
        exp: i64::MAX,
        jti: "development".to_string(),
        name: identity.development_principal.name.clone(),
        email: identity.development_principal.email.clone(),
        groups: identity.development_principal.groups.clone(),
        extra: Default::default(),
    }
}

/// A refusal, as a platform should answer it.
///
/// Always 401: every refusal here means the credential is wrong, never that
/// the principal lacks permission. A 403 is the platform's own decision, taken
/// after this succeeds. Conflating the two makes a broken deployment look like
/// a permissions problem, and nobody investigates permissions
/// ([[HLIN-S-0004]]).
fn refuse(refusal: Refusal) -> Response {
    tracing::warn!(reason = %refusal, "refused an identity");
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "identity was not accepted" })),
    )
        .into_response()
}

/// A refusal for this request, with the reason in the body.
///
/// Still always 401, for the same reason as [`refuse`].
fn refuse_request(refusal: RequestRefusal) -> Response {
    tracing::warn!(reason = %refusal, "refused an identity for this request");
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": "identity was not accepted",
            "reason": refusal.to_string(),
        })),
    )
        .into_response()
}

pub use axum::extract::FromRef;
