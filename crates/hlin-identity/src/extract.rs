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
                Ok(HlinIdentity(crate::Claims {
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
                }))
            }

            #[cfg(not(feature = "dev-identity"))]
            None => Err(refuse(Refusal::Absent)),
        }
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

pub use axum::extract::FromRef;
