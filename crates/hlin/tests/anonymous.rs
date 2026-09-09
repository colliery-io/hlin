//! An open shell: nobody signs in, and nothing can be written.
//!
//! Over the real router, because every claim here is about routing and
//! extraction — which extractor a handler asked for, what a layer put on the
//! way in, and what came back on the way out. A test of the functions
//! underneath would pass with the router wired wrong.
//!
//! The claims, from [[HLIN-A-0012]]:
//!
//! - A release build may use this strategy; `dev` may not.
//! - A visitor with no cookie is answered, and given one.
//! - Two visitors are two principals, so two surfaces. This is the claim a
//!   single-browser test cannot make and the one that fails silently in
//!   production if the identity is shared.
//! - Every write is refused, with a reason.
//! - Reads and interaction are untouched.

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hlin::config::{AuthConfig, Config, CookieConfig, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, NewLayout, Store};
use hlin_manifest::Manifest;
use tower::ServiceExt;

const PLATFORM: &str = "orebank";

struct Everywhere;

#[async_trait]
impl ManifestClient for Everywhere {
    async fn fetch(&self, _base_url: &str) -> Fetched {
        Fetched::Document(Box::new(manifest()))
    }
}

fn manifest() -> Manifest {
    hlin_manifest::parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": {{ "id": "{PLATFORM}", "name": "Orebank" }},
          "panels": [
            {{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
               "envelope": "series.v1", "data": "api/throughput" }}
          ],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

fn anonymous() -> Config {
    Config {
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::Anonymous {
            cookie: CookieConfig {
                name: "hlin_visitor".to_string(),
                secure: false,
                same_site: "Lax".to_string(),
            },
            name: None,
        },
        timings: Timings::default(),
        platforms: vec![PlatformConfig {
            id: PLATFORM.to_string(),
            base_url: "http://127.0.0.1:9999".to_string(),
            auth: CredentialConfig::None,
        }],
    }
}

async fn shell_with(configured: Config) -> (axum::Router, Arc<dyn Store>) {
    let config = Arc::new(configured);
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Everywhere))
            .expect("registry builds"),
    );
    registry.poll_all().await;

    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store: store.clone(),
        client: reqwest::Client::new(),
        stream_client: reqwest::Client::new(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    (router(state), store)
}

/// A request, optionally carrying a visitor cookie.
fn get(path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    builder.body(Body::empty()).expect("a request")
}

fn send(method: &str, path: &str, cookie: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("a request")
}

async fn call(
    app: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, serde_json::Value, Option<String>) {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let set_cookie = response
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a readable body");
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body, set_cookie)
}

/// `name=value` from a `Set-Cookie`, for sending back.
fn as_cookie(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("a set-cookie has a value")
        .to_string()
}

// -- Starting up -----------------------------------------------------------

/// The whole point. `dev` is refused outside a debug build because it makes
/// every request the same person *and* lets that person write; this gives
/// every visitor their own identity and refuses every write, so it is safe in
/// a way `dev` is not — see HLIN-A-0012.
#[test]
fn a_release_build_may_use_it() {
    assert!(anonymous().check().is_ok());
    assert_eq!(anonymous().auth.name(), "anonymous");
}

#[test]
fn it_is_the_only_strategy_that_refuses_writes() {
    assert!(anonymous().read_only());

    let mut with_a_proxy = anonymous();
    with_a_proxy.auth = AuthConfig::TrustedHeader {
        header: "x-forwarded-user".to_string(),
        groups_header: None,
        name_header: None,
        acknowledge_proxy_required: true,
    };
    assert!(!with_a_proxy.read_only());
}

/// A cookie with no name is a cookie no browser can be given.
#[test]
fn a_nameless_cookie_is_refused() {
    let mut broken = anonymous();
    broken.auth = AuthConfig::Anonymous {
        cookie: CookieConfig {
            name: "  ".to_string(),
            secure: false,
            same_site: "Lax".to_string(),
        },
        name: None,
    };
    assert!(broken.check().is_err());
}

// -- Being given a name ----------------------------------------------------

/// The first request of a visit must be *answered*, not refused and retried.
/// A browser fetches its configuration before anything else, so a shell that
/// only named a visitor on the second request would show that refusal first.
#[tokio::test]
async fn a_visitor_with_no_cookie_is_answered_and_given_one() {
    let (app, _) = shell_with(anonymous()).await;

    let (status, body, set_cookie) = call(&app, get("/api/config", None)).await;

    assert_eq!(status, StatusCode::OK);
    let cookie = set_cookie.expect("a visitor is given a name");
    assert!(cookie.starts_with("hlin_visitor="), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");

    // And the request it was minted on already knew who it was.
    let sub = body["principal"]["sub"].as_str().expect("a subject");
    assert!(sub.starts_with("anonymous:"), "{sub}");
    assert_eq!(body["read_only"], serde_json::json!(true));
}

#[tokio::test]
async fn the_same_cookie_is_the_same_person() {
    let (app, _) = shell_with(anonymous()).await;

    let (_, _, minted) = call(&app, get("/api/config", None)).await;
    let cookie = as_cookie(&minted.expect("a name"));

    let (status, first, again) = call(&app, get("/api/config", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        again.is_none(),
        "a visitor who has a name is not given another"
    );

    let (_, second, _) = call(&app, get("/api/config", Some(&cookie))).await;
    assert_eq!(first["principal"]["sub"], second["principal"]["sub"]);
}

/// The claim a single browser cannot make. If this fails, every visitor shares
/// one live surface, and one person changing a filter moves everybody else's
/// charts — in production, and only once a second person is looking.
#[tokio::test]
async fn two_visitors_are_two_people() {
    let (app, _) = shell_with(anonymous()).await;

    let (_, ada, _) = call(&app, get("/api/config", None)).await;
    let (_, grace, _) = call(&app, get("/api/config", None)).await;

    let ada = ada["principal"]["sub"].as_str().expect("a subject");
    let grace = grace["principal"]["sub"].as_str().expect("a subject");

    assert_ne!(ada, grace);
}

// -- Refusing to write -----------------------------------------------------

/// Every route that changes something, refused, with a reason rather than a
/// bare status. Table-driven so a route added without a thought about this
/// shows up as a gap here rather than as a way in.
#[tokio::test]
async fn every_write_is_refused() {
    let (app, store) = shell_with(anonymous()).await;

    // Something to aim the id-bearing routes at, so a 403 cannot be a 404 in
    // disguise.
    let existing = store
        .create_layout(NewLayout::personal("somebody", "Theirs".to_string()))
        .await
        .expect("the store accepts a layout");
    let id = existing.id;

    let (_, _, minted) = call(&app, get("/api/config", None)).await;
    let cookie = as_cookie(&minted.expect("a name"));

    let writes = [
        ("POST", "/api/layouts".to_string(), r#"{"title":"Mine"}"#),
        (
            "PUT",
            format!("/api/layouts/{id}"),
            r#"{"id":"x","title":"Mine","panels":[]}"#,
        ),
        ("DELETE", format!("/api/layouts/{id}"), ""),
        ("POST", format!("/api/layouts/{id}/fork"), ""),
    ];

    for (method, path, body) in writes {
        let (status, answer, _) = call(&app, send(method, &path, Some(&cookie), body)).await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(
            answer["error"],
            serde_json::json!("read_only"),
            "{method} {path}"
        );
        assert!(
            answer["detail"].as_str().unwrap_or("").len() > 40,
            "{method} {path} must say why, not merely refuse"
        );
    }
}

/// The guard is about the strategy, not about the route. Without this, a test
/// suite could be passing because the routes are broken for everybody.
#[tokio::test]
async fn the_same_writes_work_under_an_authenticator() {
    let mut with_a_proxy = anonymous();
    with_a_proxy.auth = AuthConfig::TrustedHeader {
        header: "x-forwarded-user".to_string(),
        groups_header: None,
        name_header: None,
        acknowledge_proxy_required: true,
    };
    let (app, _) = shell_with(with_a_proxy).await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/layouts")
        .header("content-type", "application/json")
        .header("x-forwarded-user", "ada")
        .body(Body::from(r#"{"title":"Mine"}"#))
        .expect("a request");

    let (status, _, _) = call(&app, request).await;
    assert_eq!(status, StatusCode::CREATED);
}

// -- What still works ------------------------------------------------------

/// Interaction is not a write. A time range and a filter selection live in the
/// in-memory surface, keyed per visitor, so an open instance is something a
/// person can actually use rather than a picture of one.
#[tokio::test]
async fn reading_and_looking_around_still_work() {
    let (app, _) = shell_with(anonymous()).await;

    let (_, _, minted) = call(&app, get("/api/config", None)).await;
    let cookie = as_cookie(&minted.expect("a name"));

    for path in ["/api/platforms", "/api/panels", "/api/layouts"] {
        let (status, _, _) = call(&app, get(path, Some(&cookie))).await;
        assert_eq!(status, StatusCode::OK, "{path}");
    }
}

/// A visitor lands on what was published. Creating one per visitor would be a
/// write, and an unbounded one — a row for every browser that ever arrives.
#[tokio::test]
async fn home_shows_what_was_published_and_never_creates() {
    let (app, store) = shell_with(anonymous()).await;

    let (_, _, minted) = call(&app, get("/api/config", None)).await;
    let cookie = as_cookie(&minted.expect("a name"));

    // Nothing published yet: said plainly, rather than answered with a new
    // empty surface nobody asked for.
    let (status, body, _) = call(&app, get("/api/layouts/home", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let said = body["error"].as_str().unwrap_or_default().to_string()
        + body["reason"].as_str().unwrap_or_default();
    assert!(said.contains("published"), "{body}");

    let before = store.published_layouts().await.expect("a store").len();

    let published = store
        .create_layout(NewLayout {
            visibility: hlin::store::types::Visibility::Published,
            ..NewLayout::personal("operator", "The wall".to_string())
        })
        .await
        .expect("the store accepts a layout");

    let (status, body, _) = call(&app, get("/api/layouts/home", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], serde_json::json!(published.id.to_string()));

    // And nothing was created along the way.
    let after = store.published_layouts().await.expect("a store").len();
    assert_eq!(after, before + 1, "only the one an operator published");
}

/// A regression, and a costly one for how quiet it was.
///
/// `/api/config` is what tells a browser this shell's staleness grace, its
/// refresh cadence, and now whether it may write. `PrincipalSummary::name` was
/// a bare `String` while the shell has always sent `null` for a principal with
/// no display name — so the whole document failed to deserialise for every such
/// principal, and a browser's only response to a failed fetch is to keep its
/// built-in defaults. Nothing appeared on screen. Nothing appeared in a log.
///
/// It bit `trusted-header` with no `name_header` and `oidc` against a provider
/// that returns no `name` claim; under `anonymous` it is *every* visitor, which
/// is the only reason it was found.
#[tokio::test]
async fn a_nameless_principal_still_gets_a_readable_config() {
    let (app, _) = shell_with(anonymous()).await;

    let (status, body, _) = call(&app, get("/api/config", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["principal"]["name"].is_null(), "nobody is named here");

    // The browser's own type, not a hand-written expectation of it: this is
    // the deserialisation that was failing.
    let parsed: hlin_stream::layout::ClientConfig =
        serde_json::from_value(body).expect("a browser can read this shell's configuration");

    assert!(parsed.read_only);
    assert_eq!(parsed.principal.name, None);
    assert!(parsed.stream_loss_grace_seconds > 0);
}
