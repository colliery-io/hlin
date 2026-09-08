//! Signing in, when the shell does it itself.
//!
//! Only `oidc` needs any of this. `dev` invents a principal and
//! `trusted-header` is told one by a proxy: both answer "who is this?" from the
//! request in front of them and have nothing to remember. `oidc` is the
//! strategy where the shell is the one asking, which means a round trip to a
//! provider, a session to show for it, and somewhere to keep both.
//!
//! Three routes and a sweeper. The protocol itself is in [`oidc`]; this module
//! is the part that belongs to the shell — what a session is, how the cookie is
//! written, and where a person lands afterwards.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use chrono::Utc;
use serde::Deserialize;

use crate::config::{AuthConfig, CALLBACK_PATH, LOGIN_PATH, OidcConfig};
use crate::server::AppState;
use crate::store::types::{PendingLogin, Session};

pub mod oidc;

/// How long a person has to finish signing in once they have started.
///
/// Long enough for a password manager, a second factor and a moment of
/// confusion; short enough that an abandoned sign-in is not still redeemable
/// when somebody finds the address in a history file tomorrow.
const SIGN_IN_WINDOW_MINUTES: i64 = 10;

/// How often expired sessions and abandoned sign-ins are deleted.
const SWEEP_MINUTES: u64 = 15;

/// The sign-in routes, where the strategy has any.
///
/// Returned as a router rather than merged into the main one unconditionally,
/// because a shell using `dev` or `trusted-header` should not answer on
/// `/auth/login` at all: a route that exists and cannot work is worse than one
/// that is not there, and 404 is the honest answer to "sign me in" from a shell
/// that does not do that.
pub fn routes(auth: &AuthConfig) -> Router<AppState> {
    if !matches!(auth, AuthConfig::Oidc(_)) {
        return Router::new();
    }

    Router::new()
        .route(LOGIN_PATH, get(login))
        .route(CALLBACK_PATH, get(callback))
        .route("/auth/logout", post(logout))
}

/// The name a session is stored under: the SHA-256 of the cookie's value, hex.
///
/// The cookie is a bearer credential — whoever holds it is the person — so a
/// dump of the sessions table would be a set of live sessions if the values
/// were stored as they are sent. The shell has the value on every request and
/// can hash it in microseconds, so storing the value itself buys nothing at all
/// and costs the whole table's worth of access.
pub fn fingerprint(cookie_value: &str) -> String {
    use sha2::Digest;

    let digest = sha2::Sha256::digest(cookie_value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A value nobody can guess, base64url, from the operating system.
///
/// Used for the cookie, the `state` and the PKCE verifier — three things whose
/// entire security is that the party who did not create them cannot produce
/// them.
pub fn unguessable() -> String {
    use base64::Engine;

    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system can produce random bytes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Where somebody wanted to go before they were asked who they were.
#[derive(Debug, Deserialize)]
struct Next {
    #[serde(default)]
    next: Option<String>,
}

/// Start a sign-in.
async fn login(State(app): State<AppState>, Query(next): Query<Next>) -> Response {
    let Some(oidc) = oidc_config(&app) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let provider = match oidc::Provider::discover(&app.client, oidc).await {
        Ok(provider) => provider,
        Err(reason) => {
            tracing::error!(%reason, "the identity provider could not be reached");
            return (
                StatusCode::BAD_GATEWAY,
                "the identity provider could not be reached",
            )
                .into_response();
        }
    };

    let state = unguessable();
    let nonce = unguessable();
    let verifier = unguessable();

    let pending = PendingLogin {
        state: state.clone(),
        nonce: nonce.clone(),
        code_verifier: verifier.clone(),
        redirect_to: somewhere_on_this_shell(next.next.as_deref()),
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::minutes(SIGN_IN_WINDOW_MINUTES),
    };

    if let Err(error) = app.store.begin_login(pending).await {
        tracing::error!(%error, "a sign-in could not be recorded");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    Redirect::to(&provider.authorization_url(oidc, &state, &nonce, &verifier)).into_response()
}

/// What the provider sends back.
#[derive(Debug, Deserialize)]
struct Returned {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    /// The provider refusing, rather than answering.
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Finish a sign-in.
async fn callback(State(app): State<AppState>, Query(returned): Query<Returned>) -> Response {
    let Some(oidc) = oidc_config(&app) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // The provider declining is an ordinary outcome — a person pressed cancel,
    // or is not entitled to this application — and saying so is better than the
    // blank refusal every other failure below produces.
    if let Some(error) = returned.error {
        let detail = returned.error_description.unwrap_or_default();
        tracing::info!(%error, %detail, "the provider declined a sign-in");
        return (
            StatusCode::FORBIDDEN,
            format!("the identity provider declined this sign-in: {error} {detail}"),
        )
            .into_response();
    }

    let (Some(code), Some(state)) = (returned.code, returned.state) else {
        return (StatusCode::BAD_REQUEST, "this is not a sign-in").into_response();
    };

    // Single use: the row is deleted as it is read, so a replayed callback
    // finds nothing. Everything the shell is about to check comes from here
    // rather than from the request, which is the point of having written it
    // down — a caller who could supply their own nonce could supply an id token
    // to match it.
    let pending = match app.store.claim_login(&state).await {
        Ok(Some(pending)) => pending,
        Ok(None) => {
            // Unknown, already used, or expired. A person who left a sign-in
            // half-finished and came back to it lands here, so it says what to
            // do rather than only that something was wrong.
            return (
                StatusCode::BAD_REQUEST,
                "this sign-in has already been used or has expired; start again",
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "a sign-in could not be read back");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };

    let provider = match oidc::Provider::discover(&app.client, oidc).await {
        Ok(provider) => provider,
        Err(reason) => {
            tracing::error!(%reason, "the identity provider could not be reached");
            return (
                StatusCode::BAD_GATEWAY,
                "the identity provider could not be reached",
            )
                .into_response();
        }
    };

    let claims = match provider
        .redeem(
            &app.client,
            oidc,
            &code,
            &pending.code_verifier,
            &pending.nonce,
        )
        .await
    {
        Ok(claims) => claims,
        Err(reason) => {
            // Deliberately not shown to the browser. Every one of these means
            // something the person cannot fix, and the ones that are not
            // accidents are somebody probing what the shell will accept.
            tracing::warn!(%reason, "a sign-in was refused");
            return (StatusCode::FORBIDDEN, "this sign-in could not be verified").into_response();
        }
    };

    let value = unguessable();
    let session = Session {
        id: fingerprint(&value),
        subject: claims.subject,
        name: claims.name,
        groups: claims.groups,
        created_at: Utc::now(),
        expires_at: Utc::now() + oidc.session_life(),
    };
    let life = oidc.session_life();

    if let Err(error) = app.store.create_session(session).await {
        tracing::error!(%error, "a session could not be stored");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let mut response = Redirect::to(&pending.redirect_to).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie(oidc, &value, life.num_seconds())
            .parse()
            .expect("a cookie built from checked configuration is a valid header"),
    );
    response
}

/// End a session.
///
/// A `POST`, because a `GET` that ends a session can be triggered by any page
/// that can get the browser to load an image.
async fn logout(State(app): State<AppState>, headers: HeaderMap) -> Response {
    let Some(oidc) = oidc_config(&app) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // The row goes, not just the cookie. That is the whole reason sessions are
    // stored: a copy of the cookie taken before this moment stops working.
    if let crate::config::Asking::Session(value) = app.config.principal_from(&headers)
        && let Err(error) = app.store.end_session(&fingerprint(&value)).await
    {
        tracing::error!(%error, "a session could not be ended");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie(oidc, "", 0)
            .parse()
            .expect("a cookie built from checked configuration is a valid header"),
    );
    response
}

/// The `Set-Cookie` value for a session, or for taking one away.
///
/// `HttpOnly` throughout: nothing in the frontend has any use for the value,
/// and a session a script can read is a session any script on the page can
/// take.
fn cookie(oidc: &OidcConfig, value: &str, max_age: i64) -> String {
    let mut cookie = format!(
        "{}={value}; Path=/; HttpOnly; SameSite={}; Max-Age={max_age}",
        oidc.cookie.name, oidc.cookie.same_site
    );
    if oidc.cookie.secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// A destination on this shell, whatever was asked for.
///
/// An open redirect is a real vulnerability and the login route is exactly
/// where they are found: a link to a shell somebody trusts, which quietly
/// finishes somewhere else. So only a path is accepted, and `//host` — which is
/// a path to a browser and another origin to a person reading it — is not one.
fn somewhere_on_this_shell(asked: Option<&str>) -> String {
    match asked {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/".to_string(),
    }
}

fn oidc_config(app: &AppState) -> Option<&OidcConfig> {
    match &app.config.auth {
        AuthConfig::Oidc(oidc) => Some(oidc),
        _ => None,
    }
}

/// Delete expired sessions and abandoned sign-ins, forever.
///
/// Expiry is applied on read as well, so this is housekeeping rather than
/// enforcement: without it the tables grow with every sign-in ever made and
/// never shrink.
pub fn sweep(store: Arc<dyn crate::store::Store>) {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(SWEEP_MINUTES * 60));
        loop {
            every.tick().await;
            match store.sweep_expired().await {
                Ok(0) => {}
                Ok(gone) => tracing::debug!(gone, "swept expired sessions"),
                // Worth a line and not worth stopping for: the next tick tries
                // again, and nothing is broken in the meantime because expiry
                // is enforced on read.
                Err(error) => tracing::warn!(%error, "could not sweep expired sessions"),
            }
        }
    });
}
