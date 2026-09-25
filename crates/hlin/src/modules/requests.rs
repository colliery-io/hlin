//! Carrying a module's requests to its own platform, as the person looking
//! (decision HLIN-A-0013, specification HLIN-S-0007 *The request proxy*).
//!
//! A module has no network. When it wants something from its platform it asks
//! the shell's page over the bridge, and the page sends the request here, to
//! `/p/{platform}/{path}`, with the viewer's session. The shell then does what
//! it already does for panel data — calls the platform as itself, on the
//! viewer's behalf — with one difference that shapes everything below: the
//! path came from outside. The options route avoided that by looking every
//! address up in the manifest; here the module names the path, so the shell
//! instead holds it to what the platform declared under `routes`, and refuses
//! anything that could be read as a different path by the platform than by
//! the shell.
//!
//! The steps run in the specification's order, and the order is observable:
//! it decides which refusal a request that is wrong in two ways receives. A
//! request that did not come from the shell's own page learns nothing else;
//! somebody not signed in learns nothing about the platform's routes; and a
//! write is judged for who may make it only once it is known to be a write
//! somewhere the platform accepts one.
//!
//! Every refusal the shell makes carries `X-Hlin-Refusal`, so the page can
//! tell the shell's words from the platform's. Everything else is the
//! platform's own answer, passed back unchanged within limits, because the
//! platform decides and the module shows its decision in its own words.
//!
//! Nothing here retries. A lost answer to a write is the module's to retry,
//! with the same idempotency key, and only when a person asks.

use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, request::Parts};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use hlin_identity::{BoundRequest, normalise_path};
use hlin_manifest::Access;

use crate::identity::{Author, Caller, Carried, Viewer};
use crate::modules::ModuleLimits;
use crate::server::AppState;

/// The header marking an answer as the shell's own refusal rather than the
/// platform's.
pub const REFUSAL_HEADER: &str = "x-hlin-refusal";

/// The header carrying the key a module minted for one attempt at a write.
pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";

/// The request headers a module may send its platform.
///
/// Only what describes the body and what the module will accept back.
/// Everything else the page's request carries — its cookies above all — is
/// the shell's, not the platform's, and the platform's identity comes from the
/// credentialer instead.
const REQUEST_HEADERS: [&str; 4] = ["content-type", "accept", "if-match", "if-none-match"];

/// The response headers a platform may send its module.
///
/// Enough to read the body and to cache it. Nothing that could set a cookie on
/// the shell's origin, redirect the page, or change how the shell's page is
/// framed or secured: the answer is served from the shell's origin, so any
/// header let through is one the platform sets on the shell.
///
/// `Location` in particular. The shell does not follow a platform's redirect
/// (see [`crate::clients::Clients::proxying`]), and passes the status back as
/// the platform's answer, but without the address: the page's `fetch` follows
/// a redirect that names one, from the shell's origin with the viewer's
/// session, to wherever the platform pointed. A 3xx with no `Location` is
/// simply an answer, which the module can read and act on in its own UI.
const RESPONSE_HEADERS: [&str; 4] = ["content-type", "etag", "last-modified", "cache-control"];

/// A refusal the shell makes itself, before or instead of the platform
/// answering.
#[derive(Debug)]
pub struct Refusal {
    code: &'static str,
    status: StatusCode,
    reason: String,
}

impl Refusal {
    fn new(code: &'static str, status: StatusCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            status,
            reason: reason.into(),
        }
    }

    fn not_from_shell(reason: &str) -> Self {
        Self::new("not_from_shell", StatusCode::FORBIDDEN, reason)
    }

    fn not_signed_in() -> Self {
        Self::new(
            "not_signed_in",
            StatusCode::UNAUTHORIZED,
            "nobody is signed in",
        )
    }

    fn outside_prefix(reason: impl Into<String>) -> Self {
        Self::new("outside_prefix", StatusCode::NOT_FOUND, reason)
    }

    fn too_large(reason: impl Into<String>) -> Self {
        Self::new("too_large", StatusCode::PAYLOAD_TOO_LARGE, reason)
    }

    fn unreachable(reason: impl Into<String>) -> Self {
        Self::new("unreachable", StatusCode::BAD_GATEWAY, reason)
    }

    /// The refusal's code, as it appears in `X-Hlin-Refusal`.
    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            axum::Json(serde_json::json!({ "refusal": self.code, "reason": self.reason })),
        )
            .into_response();
        response.headers_mut().insert(
            HeaderName::from_static(REFUSAL_HEADER),
            HeaderValue::from_static(self.code),
        );
        response
    }
}

/// `{METHOD} /p/{platform}/{path}?{query}`
///
/// Takes the whole request rather than extractors, because extractors run
/// before the handler in an order the signature decides, and the order of the
/// checks here is the specification's.
pub async fn carry(State(state): State<AppState>, request: Request) -> Response {
    match carried(&state, request).await {
        Ok(response) => response,
        Err(refused) => refused,
    }
}

async fn carried(state: &AppState, request: Request) -> Result<Response, Response> {
    let (mut parts, body) = request.into_parts();
    let access = access_of(&parts.method);

    // 1. The caller. Only the shell's own page may use this route: a module
    // cannot reach it (it has no network), and nothing else should be able to
    // borrow a signed-in person's session to call a platform through it.
    check_caller(state, &parts.headers, access).map_err(IntoResponse::into_response)?;

    // 2. The person.
    let Caller(principal) = match Caller::from_request_parts(&mut parts, state).await {
        Ok(caller) => caller,
        Err(refused) if refused.status() == StatusCode::UNAUTHORIZED => {
            return Err(Refusal::not_signed_in().into_response());
        }
        // The store failing is not the viewer's fault and is not a refusal of
        // theirs; it keeps the status that says so.
        Err(other) => return Err(other),
    };

    // 3. The path.
    let (platform_id, raw_path) = split_route(parts.uri.path())
        .ok_or_else(|| Refusal::outside_prefix("no platform path").into_response())?;
    let target = target(state, &platform_id, &raw_path, access)
        .await
        .map_err(IntoResponse::into_response)?;

    // 4. The method. Checked after the path so that a path the platform never
    // declared says so whatever it was asked with, rather than telling a
    // caller which methods exist somewhere it cannot reach.
    let Some(access) = access else {
        return Err(Refusal::new(
            "method",
            StatusCode::METHOD_NOT_ALLOWED,
            format!("`{}` is neither a read nor a write", parts.method),
        )
        .into_response());
    };

    // 5. Writes.
    let idempotency_key = match access {
        Access::Read => None,
        Access::Write => Some(check_write(state, &mut parts, &target).await?),
    };

    // The body is read before identity is minted rather than while calling,
    // though the specification bounds it at the call. A bound token lives
    // thirty seconds, and a page sending its body slowly would otherwise spend
    // them before the platform saw it, and the platform's refusal of an
    // expired token would reach the operator as a fault in the shell's key.
    let limits = state.config.module_limits(&platform_id);
    let body = match access {
        Access::Read => bytes::Bytes::new(),
        Access::Write => read_request_body(&parts.headers, body, &limits)
            .await
            .map_err(IntoResponse::into_response)?,
    };

    // 6. Identity.
    let viewer = Viewer {
        principal,
        carried: Carried::from_cookie_header(
            parts
                .headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok()),
        ),
    };
    let credential = credential(state, &platform_id, &target, &parts.method, &viewer)
        .await
        .map_err(IntoResponse::into_response)?;

    // 7. The call. On the client that never follows a redirect: the prefixes
    // were checked against this path and the identity bound to it, and a
    // redirect followed would take both somewhere else.
    let mut outgoing = state
        .proxy_client
        .request(parts.method.clone(), target.url(parts.uri.query()));
    for name in REQUEST_HEADERS {
        if let Some(value) = parts.headers.get(name) {
            outgoing = outgoing.header(name, value);
        }
    }
    if let Some(key) = idempotency_key {
        outgoing = outgoing.header(IDEMPOTENCY_HEADER, key);
    }
    for (name, value) in credential {
        outgoing = outgoing.header(name, value);
    }
    if access == Access::Write {
        outgoing = outgoing.body(body);
    }

    // Sent once. A platform that failed to answer a write may or may not have
    // acted on it, and only the module, with the person's say-so and the same
    // key, may try again.
    let answer = match outgoing.send().await {
        Ok(answer) => answer,
        Err(error) if error.is_timeout() => {
            return Err(Refusal::new(
                "timeout",
                StatusCode::GATEWAY_TIMEOUT,
                format!("`{platform_id}` did not answer in time"),
            )
            .into_response());
        }
        Err(error) => {
            tracing::debug!(platform = platform_id, %error, "a module request could not reach its platform");
            return Err(
                Refusal::unreachable(format!("`{platform_id}` could not be reached"))
                    .into_response(),
            );
        }
    };

    // 8. The answer.
    if answer.status() == StatusCode::UNAUTHORIZED {
        // The platform refused the shell's credential, not the viewer: the
        // viewer is signed in, or step 2 would have said so. That is a key or
        // configuration fault the operator must hear about, and the module
        // shows the platform's words as it would any other answer.
        tracing::warn!(
            platform = platform_id,
            credential = target.strategy,
            "a platform refused the credential the shell sent with a module's request"
        );
    }

    // Streamed responses (HLIN-T-0068) branch here: the same status and
    // headers, with the body passed through as it arrives under the streaming
    // limits instead of read whole under `response_bytes`.
    deliver_whole(answer, &limits, &platform_id)
        .await
        .map_err(IntoResponse::into_response)
}

/// Whether a method reads or writes, or is neither.
fn access_of(method: &Method) -> Option<Access> {
    match method.as_str() {
        "GET" | "HEAD" => Some(Access::Read),
        "POST" | "PUT" | "PATCH" | "DELETE" => Some(Access::Write),
        _ => None,
    }
}

/// Step 1: the request came from the shell's own page.
///
/// `Sec-Fetch-Site` is set by the browser and cannot be set by page script, so
/// `same-origin` is the browser's word that the shell's page sent it. The
/// session cookie's `SameSite` is not relied on, because an operator may
/// loosen it.
///
/// `Origin` must be the shell's whenever it is sent, and must be sent on a
/// write. A browser sends it on every write, but not on a same-origin `GET` or
/// `HEAD` (the Fetch standard omits it there), so requiring it on reads would
/// refuse every read the page ever makes.
fn check_caller(
    state: &AppState,
    headers: &HeaderMap,
    access: Option<Access>,
) -> Result<(), Refusal> {
    let site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    if site != Some("same-origin") {
        return Err(Refusal::not_from_shell(
            "only the shell's own page may call this",
        ));
    }

    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    match origin {
        Some(origin) if origin == state.config.origin() => Ok(()),
        Some(_) => Err(Refusal::not_from_shell(
            "the request came from another origin",
        )),
        None if access == Some(Access::Read) => Ok(()),
        None => Err(Refusal::not_from_shell(
            "a write must say where it came from",
        )),
    }
}

/// The platform id and the path relative to its base, as the page sent them.
///
/// Read from the URI rather than axum's path extractor, which percent-decodes
/// the wildcard: decoding is the path check's to do exactly once, and a path
/// decoded before it could hide an encoded slash.
fn split_route(uri_path: &str) -> Option<(String, String)> {
    let rest = uri_path.strip_prefix("/p/")?;
    let (platform, path) = rest.split_once('/')?;
    if platform.is_empty() {
        return None;
    }
    Some((platform.to_string(), format!("/{path}")))
}

/// Where one request is going, once the path has been checked.
struct Target {
    /// The platform's base URL.
    base_url: String,
    /// The path in normal form, as it is bound into `htu`.
    normal: String,
    /// The same path, encoded once, as it is sent.
    encoded: String,
    /// Whether the platform's credential can say who is acting.
    acts_as_viewer: bool,
    /// The credential strategy's name, for the operator's log.
    strategy: &'static str,
}

impl Target {
    fn url(&self, query: Option<&str>) -> String {
        let base = self.base_url.trim_end_matches('/');
        match query.filter(|query| !query.is_empty()) {
            Some(query) => format!("{base}{}?{query}", self.encoded),
            None => format!("{base}{}", self.encoded),
        }
    }
}

/// Step 3: the path is one the platform declared for this kind of request.
///
/// The path is decoded once and refused if it holds anything two readers
/// could disagree about. The rules are `hlin-identity`'s, the same ones a
/// platform applies when it checks `htu`, so the path matched here, the path
/// bound into the token and the path the platform checks are one path. A
/// platform the shell does not know, or whose manifest it cannot currently
/// use, has declared nothing, and is refused the same way.
///
/// A method that is neither a read nor a write must still fall under some
/// declared prefix, so that it is refused for its method only where the
/// platform declared something.
async fn target(
    state: &AppState,
    platform_id: &str,
    raw_path: &str,
    access: Option<Access>,
) -> Result<Target, Refusal> {
    let normal = normalise_path(raw_path)
        .map_err(|defect| Refusal::outside_prefix(format!("the path is unusable: {defect}")))?;
    if normal
        .trim_start_matches('/')
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'))
    {
        return Err(Refusal::outside_prefix("the path names a scheme"));
    }

    let views = state.registry.views().await;
    let Some(view) = views.get(platform_id) else {
        return Err(Refusal::outside_prefix(format!(
            "no platform `{platform_id}`"
        )));
    };

    let routes = view
        .manifest
        .as_ref()
        .filter(|_| {
            view.validation
                .as_ref()
                .is_some_and(|validation| validation.is_document_valid())
        })
        .and_then(|manifest| manifest.routes.as_ref());

    let declared = routes.is_some_and(|routes| {
        let accesses: &[Access] = match &access {
            Some(access) => std::slice::from_ref(access),
            None => &Access::ALL,
        };
        accesses.iter().any(|access| {
            routes
                .prefixes(*access)
                .iter()
                .any(|prefix| hlin_manifest::path::falls_under(prefix, &normal))
        })
    });
    if !declared {
        let kind = access.map(Access::as_str).unwrap_or("any");
        return Err(Refusal::outside_prefix(format!(
            "`{platform_id}` declares no {kind} route covering this path"
        )));
    }

    Ok(Target {
        base_url: view.config.base_url.clone(),
        encoded: encode_path(&normal),
        normal,
        acts_as_viewer: view.credentialer.acts_as_viewer(),
        strategy: view.credentialer.name(),
    })
}

/// Step 5: a write may be made, by this person, to this platform, now.
///
/// Returns the idempotency key to forward.
async fn check_write(
    state: &AppState,
    parts: &mut Parts,
    target: &Target,
) -> Result<HeaderValue, Response> {
    // The same guard every layout write goes through, so a read-only shell
    // refuses a module's writes for exactly the reason it refuses those.
    if let Err(refused) = Author::from_request_parts(parts, state).await {
        return Err(match refused.status() {
            StatusCode::FORBIDDEN => Refusal::new(
                "read_only",
                StatusCode::FORBIDDEN,
                "this shell is open to anybody, and nobody can change anything through it",
            )
            .into_response(),
            StatusCode::UNAUTHORIZED => Refusal::not_signed_in().into_response(),
            _ => refused,
        });
    }

    if !target.acts_as_viewer {
        return Err(Refusal::new(
            "no_identity",
            StatusCode::CONFLICT,
            format!(
                "this platform is reached as {}, which cannot say who is acting",
                target.strategy
            ),
        )
        .into_response());
    }

    parts
        .headers
        .get(IDEMPOTENCY_HEADER)
        .filter(|key| !key.as_bytes().iter().all(u8::is_ascii_whitespace))
        .cloned()
        .ok_or_else(|| {
            Refusal::new(
                "no_idempotency_key",
                StatusCode::BAD_REQUEST,
                "a write must carry an Idempotency-Key",
            )
            .into_response()
        })
}

/// A write's body, refused once it passes `request_bytes`.
///
/// A declared length over the limit is refused before a byte is read, and the
/// body is then counted as it arrives, for the reasons `bounded` gives for a
/// platform's answers: the length is optional and is a claim by the party
/// whose length is in question.
async fn read_request_body(
    headers: &HeaderMap,
    body: Body,
    limits: &ModuleLimits,
) -> Result<bytes::Bytes, Refusal> {
    let limit = limits.request_bytes;
    let over = || Refusal::too_large(format!("a request body may be at most {limit} bytes"));

    let declared = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|declared| declared > limit as u64) {
        return Err(over());
    }

    let mut read = Vec::new();
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        // The page went away mid-body. Nobody is left to read this.
        let chunk = chunk.map_err(|_| Refusal::unreachable("the request body ended early"))?;
        if read.len() + chunk.len() > limit {
            return Err(over());
        }
        read.extend_from_slice(&chunk);
    }
    Ok(bytes::Bytes::from(read))
}

/// Step 6: the headers that say who is asking.
///
/// A read carries what every read to this platform carries. A write carries
/// identity tied to this method and path, where the strategy can tie it.
/// Taken from the registry again rather than held since step 3, so the
/// registry is not held across reading the body.
async fn credential(
    state: &AppState,
    platform_id: &str,
    target: &Target,
    method: &Method,
    viewer: &Viewer,
) -> Result<Vec<(String, String)>, Refusal> {
    let views = state.registry.views().await;
    let Some(view) = views.get(platform_id) else {
        return Err(Refusal::outside_prefix(format!(
            "no platform `{platform_id}`"
        )));
    };

    let produced = if access_of(method) == Some(Access::Write) {
        // The path was normalised by the same rules in step 3, so binding it
        // cannot fail here; the error is carried rather than unwrapped all
        // the same.
        BoundRequest::new(method.as_str(), &target.normal)
            .map_err(|defect| defect.to_string())
            .and_then(|request| view.credentialer.write_headers(viewer, &request))
    } else {
        view.credentialer.headers(viewer)
    };

    produced.map_err(|reason| {
        tracing::warn!(platform = platform_id, %reason, "no credential for a module request");
        Refusal::new(
            "no_identity",
            StatusCode::CONFLICT,
            "the shell has no identity to send this platform for you",
        )
    })
}

/// Step 8: the platform's answer, whole, with only the headers a module may
/// see.
async fn deliver_whole(
    answer: reqwest::Response,
    limits: &ModuleLimits,
    platform_id: &str,
) -> Result<Response, Refusal> {
    let status = answer.status();
    let mut headers = HeaderMap::new();
    for name in RESPONSE_HEADERS {
        if let Some(value) = answer.headers().get(name) {
            headers.insert(HeaderName::from_static(name), value.clone());
        }
    }

    let body = crate::bounded::read_bounded(answer, limits.response_bytes)
        .await
        .map_err(|trouble| match trouble {
            crate::bounded::TooMuch::Oversized { limit, .. } => Refusal::too_large(format!(
                "`{platform_id}` answered with more than {limit} bytes"
            )),
            crate::bounded::TooMuch::Interrupted => {
                Refusal::unreachable(format!("`{platform_id}` stopped answering partway"))
            }
        })?;

    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

/// A path in normal form, percent-encoded once, as it goes on the wire.
///
/// The normal form is decoded, so it cannot be sent as it is. Encoding every
/// byte outside the characters a path segment may hold literally gives one
/// spelling per path, which the platform decodes back to exactly the normal
/// form bound into the token.
fn encode_path(normal: &str) -> String {
    let mut encoded = String::with_capacity(normal.len());
    for byte in normal.bytes() {
        let literal = byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&byte);
        if literal {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_splits_into_its_platform_and_the_path_under_it() {
        assert_eq!(
            split_route("/p/checklist/api/items"),
            Some(("checklist".to_string(), "/api/items".to_string()))
        );
        assert_eq!(split_route("/p//api"), None);
        assert_eq!(split_route("/q/checklist/api"), None);
    }

    #[test]
    fn an_encoded_path_decodes_back_to_the_path_it_was_made_from() {
        for normal in ["/api/items", "/api/a b", "/api/100%", "/api/é", "/api/x?y"] {
            let encoded = encode_path(normal);
            assert!(!encoded.contains(' ') && !encoded.contains('?'));
            assert_eq!(normalise_path(&encoded).as_deref(), Ok(normal));
        }
    }

    #[test]
    fn only_the_six_methods_have_an_access() {
        assert_eq!(access_of(&Method::GET), Some(Access::Read));
        assert_eq!(access_of(&Method::HEAD), Some(Access::Read));
        for write in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert_eq!(access_of(&write), Some(Access::Write));
        }
        assert_eq!(access_of(&Method::OPTIONS), None);
        assert_eq!(access_of(&Method::TRACE), None);
    }
}
