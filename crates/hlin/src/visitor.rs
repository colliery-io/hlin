//! Giving a visitor a name, when nobody signs in.
//!
//! Only the `anonymous` strategy needs this. Every other strategy answers "who
//! is this?" from something the request already carries — a header a proxy set,
//! a cookie the shell issued at the end of a sign-in — and has nothing to mint.
//!
//! # Why a layer and not the extractor
//!
//! [`Config::principal_from`](crate::config::Config::principal_from) is a
//! function of the headers, which is what keeps it testable and what lets
//! `Asking` mean "what the headers say" rather than "what the shell decided".
//! Minting is not that: it has to reach the *response*, because a browser that
//! is never sent a `Set-Cookie` arrives as a new visitor on every request and
//! gets a new surface each time — the time range resetting between one frame
//! and the next.
//!
//! So the mint happens here, in front of the handlers. A request with no
//! visitor cookie has one written into its own headers, so everything
//! downstream sees a request that carries it, and the value is attached to the
//! response on the way out.
//!
//! # Why this is not a session
//!
//! There is no row, no expiry sweep and nothing to revoke. The cookie names
//! nothing that is stored, because a shell using this strategy refuses every
//! write (HLIN-A-0012). Its worst loss is a stranger sharing your time range.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::config::{AuthConfig, CookieConfig};
use crate::server::AppState;

/// How long a browser keeps its name.
///
/// Long, because the thing it buys is continuity: a visitor who comes back
/// tomorrow finds the time range they left. Not forever, because a cookie with
/// no end is a cookie that outlives every reason it was set.
const KEEP_DAYS: u64 = 30;

/// Mint a visitor cookie where the request has none.
///
/// Applied only under `anonymous`; see [`layer`].
pub async fn assign(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let AuthConfig::Anonymous { cookie, .. } = &state.config.auth else {
        return next.run(request).await;
    };

    // Already named. Nothing to do, and in particular nothing to re-set: a
    // `Set-Cookie` on every response would keep resetting the expiry, which is
    // harmless, and would also mean a response that cannot be cached, which is
    // not.
    if read(&request, &cookie.name).is_some() {
        return next.run(request).await;
    }

    let minted = crate::auth::unguessable();

    // Into the request the handlers see, so `principal_from` finds it on this
    // request and not only on the next one. Without this the first request of
    // every visit is refused, and a browser that fetches its configuration
    // before anything else would see that refusal first.
    let carried = match request.headers().get(axum::http::header::COOKIE) {
        Some(existing) => format!(
            "{}; {}={minted}",
            existing.to_str().unwrap_or(""),
            cookie.name
        ),
        None => format!("{}={minted}", cookie.name),
    };
    if let Ok(value) = axum::http::HeaderValue::from_str(&carried) {
        request
            .headers_mut()
            .insert(axum::http::header::COOKIE, value);
    }

    let mut response = next.run(request).await;

    if let Ok(value) = axum::http::HeaderValue::from_str(&header_for(cookie, &minted)) {
        response
            .headers_mut()
            .append(axum::http::header::SET_COOKIE, value);
    }

    response
}

/// The `Set-Cookie` value.
///
/// `HttpOnly` because nothing in the browser reads it: the front end learns who
/// it is from `/api/config`, and a cookie script cannot touch is a cookie an
/// injected script cannot read either.
fn header_for(cookie: &CookieConfig, value: &str) -> String {
    let mut header = format!(
        "{}={value}; Path=/; HttpOnly; SameSite={}; Max-Age={}",
        cookie.name,
        cookie.same_site,
        KEEP_DAYS * 24 * 60 * 60
    );
    if cookie.secure {
        header.push_str("; Secure");
    }
    header
}

/// One cookie off a request, by name.
fn read(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie() -> CookieConfig {
        CookieConfig {
            name: "hlin_visitor".to_string(),
            secure: false,
            same_site: "Lax".to_string(),
        }
    }

    #[test]
    fn the_header_carries_what_a_browser_needs_to_keep_it() {
        let header = header_for(&cookie(), "abc");

        assert!(header.starts_with("hlin_visitor=abc;"));
        assert!(header.contains("Path=/"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Lax"));
        assert!(header.contains("Max-Age=2592000"), "thirty days");
    }

    /// Off by default so an open instance is reachable over plain HTTP, which
    /// is the case the strategy exists for. On when configured.
    #[test]
    fn secure_is_set_only_when_asked_for() {
        assert!(!header_for(&cookie(), "abc").contains("Secure"));

        let secure = CookieConfig {
            secure: true,
            ..cookie()
        };
        assert!(header_for(&secure, "abc").contains("; Secure"));
    }

    /// Two visitors are two names. If this ever fails they share a surface,
    /// and one person's click moves the other's charts.
    #[test]
    fn every_visitor_is_a_different_one() {
        let names: std::collections::HashSet<String> =
            (0..64).map(|_| crate::auth::unguessable()).collect();

        assert_eq!(names.len(), 64);
    }
}
