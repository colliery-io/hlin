//! Serving a platform's module code from the shell's own origin
//! (specification HLIN-S-0007, *Assets* and *The module CSP*).
//!
//! A module runs in a sandboxed frame whose document comes from `/m/`, so the
//! only code that frame can load is what this route agrees to serve. That makes
//! the route the containment boundary, and every rule here is about keeping it
//! narrow:
//!
//! - **Only what the platform declared.** The path asked for must fall under
//!   the platform's `assets` prefix by segment, and must pass the manifest
//!   crate's file-path rules — no `..`, no `.`, no empty segment, no backslash,
//!   no percent-encoded `/`, `\` or `.`. The path is read from the raw request
//!   URI, never from a decoded extractor, because a decoded `%2F` is exactly the
//!   separator the rule exists to refuse. Everything else is 404, with no hint
//!   of which rule it broke.
//! - **Fetched as the shell.** The shell asks the platform the way it asks for a
//!   manifest: no credential, no cookie, nothing about whoever is looking. Code
//!   is the same for every viewer, and a frame that could make the shell fetch
//!   as its viewer would be a way to read that viewer's data from outside the
//!   bridge. For the same reason the route asks for no caller: the frame's
//!   origin is opaque and sends no session, so an authenticated route would
//!   serve nothing, and what is served is only what the platform already hands
//!   the shell unauthenticated.
//! - **Bounded.** The entry document by `entry_bytes`, anything else by
//!   `asset_bytes`, read with [`crate::bounded::read_bounded`] so an oversized
//!   answer costs one refused request rather than the shell's memory.
//! - **Typed by the shell where it matters.** `.wasm` is always
//!   `application/wasm` (streaming compilation refuses anything else) and
//!   `.html` always `text/html` (the frame must be a document for its CSP to
//!   mean anything). Everything else takes the platform's type, with
//!   `nosniff`, so a browser cannot decide a stylesheet is a script.
//! - **Confined.** Every response under `/m/`, refusals included, carries the
//!   module CSP, naming this shell's origin explicitly rather than `'self'`.
//! - **Readable from the frame.** Every response also carries
//!   `Access-Control-Allow-Origin: *`. The frame's origin is opaque, so to the
//!   browser each of its own assets is cross-origin: a module script, a
//!   `modulepreload` and the `fetch` that loads a `.wasm` are all CORS
//!   requests from origin `null`, and without this header every Trunk-built
//!   module fails before its first line runs. `*` gives nothing away: these
//!   are files the shell fetched without a credential and serves without a
//!   session, to anyone who asks for them by URL.
//!
//! # Statuses, and what the frame host makes of them
//!
//! The shell page derives a panel's state from how its assets were answered
//! (*Panel states*). The statuses are chosen so the two unavailable states can
//! be told apart by class alone:
//!
//! | Answer | Status | Panel state |
//! |---|---|---|
//! | Unknown platform, no usable manifest or `assets` prefix, a path outside the prefix or breaking a path rule | 404 | `unavailable (malformed)` (a 4xx from the prefix); `unknown` is derived from the registry, not from here |
//! | The platform answered 4xx | that status | `unavailable (malformed)` |
//! | Entry or asset over its limit | 413 | `unavailable (malformed)` |
//! | The platform answered with a redirect | 502 | `unavailable (unreachable)` |
//! | The platform unreachable, answered 5xx, or failed partway through | 502 | `unavailable (unreachable)` |
//! | The platform did not answer in time | 504 | `unavailable (unreachable)` |
//!
//! So: any 4xx is malformed and any 5xx is unreachable, which is the split the
//! specification draws. 413 rather than a bare 404 for an oversized asset so an
//! operator reading a browser's network panel sees which limit to raise.
//!
//! # Caching
//!
//! The entry document is sent `no-cache`, so a browser revalidates it on every
//! mount: it is the one file whose name does not change when the module does.
//! Any other asset keeps the platform's `Cache-Control` when that says
//! `immutable`, as hashed build output does, and is `no-cache` otherwise. A
//! browser's `If-None-Match` and `If-Modified-Since` are passed on, and the
//! platform's `ETag`, `Last-Modified` and `304` passed back, so revalidating an
//! unchanged file costs a round trip rather than the file.
//!
//! # Compression
//!
//! The browser's `Accept-Encoding` is passed on too, so a platform that
//! compresses its own files (or keeps them compressed) is taken at its word:
//! its `Content-Encoding` and `Vary` come back with the bytes as they were
//! sent, and the limits bound what crossed the wire. Anything the platform
//! sent as it is, the shell compresses on the way out ([`compression`]),
//! which is what makes the twenty-widget surface a third of the bytes it was
//! ([[HLIN-T-0086]]).

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use crate::server::AppState;

/// Where module assets are served from, on the shell's origin.
pub const PREFIX: &str = "/m/";

/// The module CSP for one platform's frame (HLIN-S-0007, *The module CSP*).
///
/// `shell` is the shell's origin, `https://hlin.example.com`, with no trailing
/// slash.
pub fn module_csp(shell: &str, platform: &str) -> String {
    let own = format!("{shell}{PREFIX}{platform}/");
    [
        "default-src 'none'".to_string(),
        format!("script-src {own} 'wasm-unsafe-eval'"),
        format!("style-src {own} 'unsafe-inline'"),
        format!("img-src {own} data: blob:"),
        format!("font-src {own}"),
        format!("connect-src {own}"),
        "form-action 'none'".to_string(),
        "base-uri 'none'".to_string(),
        format!("frame-ancestors {shell}"),
    ]
    .join("; ")
}

/// The shell page's CSP (HLIN-S-0007, *The shell page's CSP*).
///
/// A frame's own navigations are checked against its embedder's policy, so
/// this is what stops a module navigating its frame to a hostile page that
/// would inherit the bridge. Only `frame-src`: the page has no other policy
/// yet, and a full one is a separate piece of work.
pub fn page_csp(shell: &str) -> String {
    format!("frame-src {shell}{PREFIX}")
}

/// What was asked for, once it has been checked against the manifest.
struct Asked {
    platform: String,
    url: reqwest::Url,
    entry: bool,
    limit: usize,
}

/// `GET /m/{platform}/{path}`
pub async fn serve(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let shell = state.config.origin_for(&headers);

    // The platform segment is needed for the CSP even on a refusal, and is
    // taken as written: it only ever names a platform to look up, and one that
    // is not configured is refused below.
    let rest = uri.path().strip_prefix(PREFIX).unwrap_or("");
    let (platform, path) = match rest.split_once('/') {
        Some((platform, path)) => (platform, format!("/{path}")),
        None => (rest, String::new()),
    };

    let answer = match look_up(&state, platform, &path).await {
        Some(asked) => fetch(&state, asked, &headers).await,
        None => refused(StatusCode::NOT_FOUND, "no such module asset"),
    };

    // Only a configured platform is named in a policy. Anything else in that
    // segment is whatever the requester wrote, and a policy is no place to
    // repeat it: a `;` there would start a directive of the requester's own.
    let known = state
        .config
        .platforms
        .iter()
        .any(|configured| configured.id == platform);
    confine(answer, &shell, known.then_some(platform))
}

/// Check the request against what the platform currently declares.
///
/// `None` for every refusal alike: which rule a path broke is not something to
/// tell whoever is probing for one it does not.
async fn look_up(state: &AppState, platform: &str, path: &str) -> Option<Asked> {
    hlin_manifest::path::check_file(path).ok()?;

    let views = state.registry.views().await;
    let view = views.get(platform)?;
    let manifest = view.manifest.as_ref()?;
    if !view
        .validation
        .as_ref()
        .is_some_and(|validation| validation.is_document_valid())
    {
        return None;
    }

    let assets = manifest.assets.as_deref()?;
    if !hlin_manifest::path::falls_under(assets, path) {
        return None;
    }

    // An entry is whatever a panel or a navigation entry names as one. Judged
    // by declaration rather than by extension, because the entry is what a
    // mount loads first, and so what must never be served stale.
    let entry = manifest
        .panels
        .iter()
        .filter_map(|panel| panel.ui.as_ref())
        .chain(
            manifest
                .navigation
                .iter()
                .filter_map(|entry| entry.ui.as_ref()),
        )
        .any(|ui| ui.entry == path);

    // Parsed here rather than handed to the client as a string, so the address
    // compared after the fetch is the one the client actually asked for.
    let url = reqwest::Url::parse(&format!(
        "{}{path}",
        view.config.base_url.trim_end_matches('/')
    ))
    .ok()?;

    let limits = state.config.module_limits(platform);
    Some(Asked {
        platform: platform.to_string(),
        url,
        entry,
        limit: if entry {
            limits.entry_bytes
        } else {
            limits.asset_bytes
        },
    })
}

/// What the browser may pass on: whether it already has the file, and which
/// encodings it can read. Neither says anything about who is looking.
const PASSED_ON: [header::HeaderName; 3] = [
    header::IF_NONE_MATCH,
    header::IF_MODIFIED_SINCE,
    header::ACCEPT_ENCODING,
];

/// What the platform's answer may pass back to a browser. `Content-Encoding`
/// because the body is passed back as it was sent, and `Vary` because a
/// platform that chose an encoding by `Accept-Encoding` says so there.
const PASSED_BACK: [header::HeaderName; 4] = [
    header::ETAG,
    header::LAST_MODIFIED,
    header::CONTENT_ENCODING,
    header::VARY,
];

/// Compression for what the shell sends a browser that it did not get
/// compressed: module assets here, and the shell's own frontend
/// ([`crate::server::with_frontend`]).
///
/// gzip, and brotli where the browser accepts it (at quality 4, as a server
/// compressing as it goes should: the default of 11 takes seconds over a
/// frontend's wasm). Nothing under a kilobyte, where the headers cost more
/// than the saving; nothing already encoded; no images, which are; and never
/// an event stream, which would be held until a compressor's buffer filled.
/// Never on `/p/`, where an answer may be streamed and every piece must reach
/// the module as it arrives: that route is outside this layer, not excluded
/// by it.
pub fn compression()
-> tower_http::compression::CompressionLayer<impl tower_http::compression::Predicate> {
    use tower_http::compression::predicate::{NotForContentType, Predicate, SizeAbove};
    tower_http::compression::CompressionLayer::new().compress_when(
        SizeAbove::new(1024)
            .and(NotForContentType::IMAGES)
            .and(NotForContentType::SSE)
            .and(NotForContentType::GRPC),
    )
}

async fn fetch(state: &AppState, asked: Asked, headers: &HeaderMap) -> Response {
    // As the shell, like a manifest: nothing from the viewer but whether the
    // browser already has this file, and what it can decompress.
    // The proxying client, which never follows a redirect: a platform's 3xx
    // is refused below before anything is fetched from where it points.
    let mut request = state.proxy_client.get(asked.url.clone());
    for name in PASSED_ON {
        if let Some(value) = headers.get(&name) {
            request = request.header(name, value);
        }
    }

    let answer = match request.send().await {
        Ok(answer) => answer,
        Err(error) if error.is_timeout() => {
            tracing::debug!(platform = asked.platform, %error, "module asset timed out");
            return refused(StatusCode::GATEWAY_TIMEOUT, "the platform did not answer");
        }
        Err(error) => {
            tracing::debug!(platform = asked.platform, %error, "module asset unreachable");
            return refused(StatusCode::BAD_GATEWAY, "the platform could not be reached");
        }
    };

    // A redirect is refused rather than followed. Following it would let a
    // platform point its prefix anywhere the shell can reach, so it counts as
    // a platform failing to serve what it declared, and the address it names
    // is never asked for. `304 Not Modified` is a 3xx too, and is not a
    // redirect: it is the answer to the browser's revalidation.
    if answer.status().is_redirection() && answer.status() != StatusCode::NOT_MODIFIED {
        tracing::warn!(
            platform = asked.platform,
            asked = %asked.url,
            "module asset redirected; refused"
        );
        return refused(StatusCode::BAD_GATEWAY, "the platform redirected");
    }

    let status = answer.status();
    let mut passed = HeaderMap::new();
    for name in PASSED_BACK {
        if let Some(value) = answer.headers().get(&name) {
            passed.insert(name, value.clone());
        }
    }
    let cache = cache_control(asked.entry, answer.headers());

    if status == StatusCode::NOT_MODIFIED {
        passed.insert(header::CACHE_CONTROL, cache);
        return (StatusCode::NOT_MODIFIED, passed).into_response();
    }
    if status.is_server_error() {
        return refused(StatusCode::BAD_GATEWAY, "the platform failed");
    }
    if !status.is_success() {
        // A 4xx says the platform does not have what it declared, which is the
        // module's fault rather than the network's; passed on as it came.
        let status = if status.is_client_error() {
            status
        } else {
            StatusCode::BAD_GATEWAY
        };
        return refused(status, "the platform refused");
    }

    let content_type = content_type(asked.url.path(), answer.headers());
    let body = match crate::bounded::read_bounded(answer, asked.limit).await {
        Ok(body) => body,
        Err(crate::bounded::TooMuch::Oversized { bytes, limit }) => {
            tracing::warn!(
                platform = asked.platform,
                asset = %asked.url,
                bytes,
                limit,
                entry = asked.entry,
                "module asset over its limit"
            );
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "over the module asset limit");
        }
        Err(crate::bounded::TooMuch::Interrupted) => {
            return refused(StatusCode::BAD_GATEWAY, "the platform stopped partway");
        }
    };

    passed.insert(header::CONTENT_TYPE, content_type);
    passed.insert(header::CACHE_CONTROL, cache);
    (StatusCode::OK, passed, Body::from(body)).into_response()
}

/// How long a browser may keep what it was sent.
fn cache_control(entry: bool, platform: &HeaderMap) -> HeaderValue {
    let no_cache = HeaderValue::from_static("no-cache");
    if entry {
        return no_cache;
    }
    match platform.get(header::CACHE_CONTROL) {
        Some(value)
            if value.to_str().is_ok_and(|said| {
                said.split(',')
                    .any(|directive| directive.trim().eq_ignore_ascii_case("immutable"))
            }) =>
        {
            value.clone()
        }
        _ => no_cache,
    }
}

/// The type an asset is served as.
fn content_type(path: &str, platform: &HeaderMap) -> HeaderValue {
    let lowered = path.to_ascii_lowercase();
    if lowered.ends_with(".wasm") {
        return HeaderValue::from_static("application/wasm");
    }
    if lowered.ends_with(".html") {
        return HeaderValue::from_static("text/html; charset=utf-8");
    }
    platform
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"))
}

fn refused(status: StatusCode, reason: &'static str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        reason,
    )
        .into_response()
}

/// Put the module CSP on an answer, whatever it was.
///
/// With no platform, the policy that allows nothing at all: there is no
/// module to confine to itself, only a refusal to keep inert.
fn confine(mut response: Response, shell: &str, platform: Option<&str>) -> Response {
    let headers = response.headers_mut();
    let csp = platform
        .and_then(|platform| HeaderValue::from_str(&module_csp(shell, platform)).ok())
        .or_else(|| {
            HeaderValue::from_str(&format!("default-src 'none'; frame-ancestors {shell}")).ok()
        })
        .unwrap_or_else(|| HeaderValue::from_static("default-src 'none'"));
    headers.insert(header::CONTENT_SECURITY_POLICY, csp);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}
