//! Carrying a module's requests to its own platform (HLIN-S-0007, *The request
//! proxy*).
//!
//! Over the real router and a real platform on a loopback socket, because
//! every claim here is about what crosses a boundary: which requests the shell
//! refuses and with what, which headers reach the platform, what identity it
//! receives, and what comes back. Each step of the specification has its own
//! section, in the specification's order, and every refusal code the shell can
//! produce is asserted at least once.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use hlin::config::{AuthConfig, Config, CookieConfig, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::modules::{LimitOverrides, PlatformModules};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use hlin_identity::{IDENTITY_HEADER, Issuer, Verifier};
use tower::ServiceExt;

const ORIGIN: &str = "https://hlin.example.com";
const USER: &str = "x-forwarded-user";

/// A small limit for the platforms that test limits, at the floor the shell
/// allows.
const SMALL: usize = 1024;

// -- A platform --------------------------------------------------------------

/// One request as the platform received it.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    /// The path as it arrived, including the platform's own prefix.
    path: String,
    query: Option<String>,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

type Log = Arc<Mutex<Vec<Seen>>>;

/// A platform that records everything and answers by the last segment of the
/// path, so one server can play every platform: each is mounted under its own
/// id, as a path-prefixed platform would be.
///
/// `/redirect` sends the caller to `elsewhere`, which is some other server.
async fn platform(elsewhere: &str) -> (String, Log) {
    let log: Log = Arc::default();
    let recorded = log.clone();
    let elsewhere = elsewhere.to_string();

    let app = axum::Router::new().fallback(move |request: Request<Body>| {
        let log = recorded.clone();
        let elsewhere = elsewhere.clone();
        async move {
            let (parts, body) = request.into_parts();
            let body = axum::body::to_bytes(body, 1 << 20)
                .await
                .unwrap_or_default()
                .to_vec();
            let path = parts.uri.path().to_string();
            log.lock().unwrap().push(Seen {
                method: parts.method.to_string(),
                path: path.clone(),
                query: parts.uri.query().map(str::to_string),
                headers: parts.headers.clone(),
                body,
            });

            match path.rsplit('/').next().unwrap_or("") {
                "forbidden" => (
                    StatusCode::FORBIDDEN,
                    [("content-type", "text/plain")],
                    "Only Alice may edit Alice's items",
                )
                    .into_response(),
                "broken" => (StatusCode::INTERNAL_SERVER_ERROR, "it broke").into_response(),
                "refused" => (StatusCode::UNAUTHORIZED, "who are you").into_response(),
                "big" => "x".repeat(SMALL * 2).into_response(),
                "redirect" => (
                    StatusCode::TEMPORARY_REDIRECT,
                    [
                        ("location", format!("{elsewhere}/stolen")),
                        ("cache-control", "no-store".to_string()),
                    ],
                    "moved",
                )
                    .into_response(),
                "slow" => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    "late".into_response()
                }
                _ => (
                    StatusCode::OK,
                    [
                        ("content-type", "application/json"),
                        ("etag", "\"v1\""),
                        ("last-modified", "Wed, 23 Sep 2026 10:00:00 GMT"),
                        ("cache-control", "no-store"),
                        ("set-cookie", "platform=owned; Path=/"),
                        ("x-internal", "secret"),
                        ("location", "https://elsewhere.example"),
                    ],
                    r#"{"ok":true}"#,
                )
                    .into_response(),
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("a port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://127.0.0.1:{port}"), log)
}

/// Every platform declares the same routes: reads under `/api/` and
/// `/reports/`, writes under `/api/` and `/actions/`. So `/reports/` is
/// read-only and `/actions/` write-only.
struct Manifests;

#[async_trait]
impl ManifestClient for Manifests {
    async fn fetch(&self, base_url: &str) -> Fetched {
        let id = base_url.trim_end_matches('/').rsplit('/').next().unwrap();
        Fetched::Document(Box::new(
            hlin_manifest::parse_str(&format!(
                r#"{{
                  "schema_version": 1,
                  "contract_version": "1.0.0",
                  "platform": {{ "id": "{id}", "name": "{id}" }},
                  "panels": [
                    {{ "key": "items", "title": "Items", "kind": "table",
                       "envelope": "records.v1", "data": "/panels/items" }}
                  ],
                  "routes": {{ "read": ["/api/", "/reports/"], "write": ["/api/", "/actions/"] }},
                  "health": "/health"
                }}"#
            ))
            .expect("fixture parses"),
        ))
    }
}

// -- A shell -----------------------------------------------------------------

struct Shell {
    app: axum::Router,
    issuer: Arc<Issuer>,
    log: Log,
    /// What the server a platform redirects to received.
    elsewhere: Log,
}

fn platform_config(id: &str, base: &str, auth: CredentialConfig) -> PlatformConfig {
    PlatformConfig {
        id: id.to_string(),
        base_url: format!("{base}/{id}"),
        auth,
        modules: Default::default(),
    }
}

/// A shell in front of one platform per credential strategy, plus one with
/// small limits and one nobody can reach.
async fn shell_with(auth: AuthConfig) -> Shell {
    // A second server, standing for anywhere a platform might point the shell.
    let (trap, elsewhere) = platform("http://127.0.0.1:1").await;
    let (base, log) = platform(&trap).await;

    // SAFETY of the test, not of the code: static-bearer reads its key from
    // the environment.
    unsafe { std::env::set_var("HLIN_TEST_PROXY_BEARER", "a-shared-key") };

    let mut small = platform_config("small", &base, CredentialConfig::HlinToken);
    small.modules = PlatformModules {
        limits: LimitOverrides {
            request_bytes: Some(SMALL),
            response_bytes: Some(SMALL),
            ..Default::default()
        },
    };

    let config = Arc::new(Config {
        public_url: Some(ORIGIN.to_string()),
        modules: Default::default(),
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth,
        // A short upstream timeout, so the slow platform is slow enough.
        timings: Timings {
            upstream_timeout_seconds: 1,
            ..Timings::default()
        },
        platforms: vec![
            platform_config("checklist", &base, CredentialConfig::HlinToken),
            platform_config("open", &base, CredentialConfig::None),
            platform_config(
                "shared",
                &base,
                CredentialConfig::StaticBearer {
                    token_env: "HLIN_TEST_PROXY_BEARER".to_string(),
                    acknowledge_shared_principal: true,
                },
            ),
            platform_config(
                "session",
                &base,
                CredentialConfig::ForwardSession {
                    cookies: vec!["platform_session".to_string()],
                },
            ),
            small,
            // Port 1: nothing listens there.
            PlatformConfig {
                id: "gone".to_string(),
                base_url: "http://127.0.0.1:1/gone".to_string(),
                auth: CredentialConfig::HlinToken,
                modules: Default::default(),
            },
        ],
    });

    let issuer = Arc::new(Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Manifests))
            .expect("registry builds"),
    );
    registry.poll_all().await;

    // The clients the shell really builds, so what is tested is its own
    // choice of timeout and redirect policy rather than the test's.
    let clients = hlin::clients::Clients::build(&config).expect("clients build");

    let state = AppState {
        config,
        registry,
        issuer: issuer.clone(),
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store,
        client: clients.fetching,
        stream_client: clients.streaming,
        proxy_client: clients.proxying,
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    Shell {
        app: router(state),
        issuer,
        log,
        elsewhere,
    }
}

async fn shell() -> Shell {
    shell_with(AuthConfig::TrustedHeader {
        header: USER.to_string(),
        groups_header: None,
        name_header: None,
        acknowledge_proxy_required: true,
    })
    .await
}

async fn read_only_shell() -> Shell {
    shell_with(AuthConfig::Anonymous {
        cookie: CookieConfig {
            name: "hlin_visitor".to_string(),
            secure: false,
            same_site: "Lax".to_string(),
        },
        name: None,
    })
    .await
}

/// A request as the shell's own page sends it: same-origin, from Alice, and
/// on a write with the shell's `Origin` and an idempotency key.
fn from_page(method: &str, uri: &str) -> axum::http::request::Builder {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("sec-fetch-site", "same-origin")
        .header(USER, "alice");
    if method == "GET" || method == "HEAD" {
        builder
    } else {
        builder
            .header("origin", ORIGIN)
            .header("idempotency-key", "01J8Z-attempt-1")
    }
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn refusal(&self) -> Option<&str> {
        self.headers
            .get("x-hlin-refusal")
            .and_then(|value| value.to_str().ok())
    }

    #[track_caller]
    fn refused(&self, status: StatusCode, code: &str) {
        assert_eq!(
            (self.status, self.refusal()),
            (status, Some(code)),
            "{}",
            String::from_utf8_lossy(&self.body)
        );
    }
}

impl Shell {
    async fn send(&self, request: axum::http::request::Builder, body: impl Into<Body>) -> Answer {
        let response = self
            .app
            .clone()
            .oneshot(request.body(body.into()).expect("a request"))
            .await
            .expect("the router answers");
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), 1 << 22)
            .await
            .expect("a body")
            .to_vec();
        Answer {
            status,
            headers,
            body,
        }
    }

    async fn call(&self, request: axum::http::request::Builder) -> Answer {
        self.send(request, Body::empty()).await
    }

    fn seen(&self) -> Vec<Seen> {
        self.log.lock().unwrap().clone()
    }

    fn only_seen(&self) -> Seen {
        let seen = self.seen();
        assert_eq!(seen.len(), 1, "the platform saw {seen:#?}");
        seen.into_iter().next().unwrap()
    }

    fn verifier(&self) -> Verifier {
        Verifier::with_keys("hlin", self.issuer.jwks())
    }
}

// -- 1. The caller -----------------------------------------------------------

#[tokio::test]
async fn a_request_without_sec_fetch_site_is_not_from_the_shell() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .uri("/p/checklist/api/items")
                .header(USER, "alice"),
        )
        .await;
    answer.refused(StatusCode::FORBIDDEN, "not_from_shell");
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_cross_site_request_is_not_from_the_shell() {
    let shell = shell().await;
    for site in ["cross-site", "same-site", "none"] {
        let answer = shell
            .call(
                Request::builder()
                    .uri("/p/checklist/api/items")
                    .header("sec-fetch-site", site)
                    .header(USER, "alice"),
            )
            .await;
        answer.refused(StatusCode::FORBIDDEN, "not_from_shell");
    }
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_request_from_another_origin_is_not_from_the_shell() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .method("POST")
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "same-origin")
                .header("origin", "https://evil.example")
                .header("idempotency-key", "k")
                .header(USER, "alice"),
        )
        .await;
    answer.refused(StatusCode::FORBIDDEN, "not_from_shell");
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_write_that_does_not_say_where_it_came_from_is_refused() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .method("POST")
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "same-origin")
                .header("idempotency-key", "k")
                .header(USER, "alice"),
        )
        .await;
    answer.refused(StatusCode::FORBIDDEN, "not_from_shell");
}

/// A browser sends no `Origin` on a same-origin `GET`, so a read without one
/// is the ordinary case and must be carried.
#[tokio::test]
async fn a_read_from_the_page_needs_no_origin() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/checklist/api/items")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.refusal(), None);
}

#[tokio::test]
async fn the_caller_is_checked_before_the_person() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "cross-site"),
        )
        .await;
    answer.refused(StatusCode::FORBIDDEN, "not_from_shell");
}

// -- 2. The person -----------------------------------------------------------

#[tokio::test]
async fn nobody_signed_in_is_refused() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "same-origin"),
        )
        .await;
    answer.refused(StatusCode::UNAUTHORIZED, "not_signed_in");
    assert!(shell.seen().is_empty());
}

/// Somebody not signed in learns nothing about which routes exist.
#[tokio::test]
async fn the_person_is_checked_before_the_path() {
    let shell = shell().await;
    let answer = shell
        .call(
            Request::builder()
                .uri("/p/checklist/admin/secrets")
                .header("sec-fetch-site", "same-origin"),
        )
        .await;
    answer.refused(StatusCode::UNAUTHORIZED, "not_signed_in");
}

// -- 3. The path -------------------------------------------------------------

#[tokio::test]
async fn a_path_outside_every_declared_prefix_is_refused() {
    let shell = shell().await;
    for path in [
        "/p/checklist/admin/secrets",
        "/p/checklist/health",
        "/p/checklist/.well-known/hlin.json",
        "/p/checklist/api",
        "/p/checklist/apix/items",
        "/p/nobody/api/items",
    ] {
        let answer = shell.call(from_page("GET", path)).await;
        answer.refused(StatusCode::NOT_FOUND, "outside_prefix");
    }
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_path_that_walks_or_hides_a_separator_is_refused() {
    let shell = shell().await;
    for path in [
        "/p/checklist/api/../admin",
        "/p/checklist/api/./items",
        "/p/checklist/api/%2e%2e/admin",
        "/p/checklist/api/%2E%2e/admin",
        "/p/checklist/api/items%2F..%2Fadmin",
        "/p/checklist/api/a%2fb",
        "/p/checklist/api/a%5Cb",
        "/p/checklist/api//items",
        "/p/checklist/api/items/",
        "/p/checklist/api/%zz",
        "/p/checklist/api/%00",
        "/p/checklist//evil.example/api/items",
        "/p/checklist/http:/evil.example/api/",
    ] {
        let answer = shell.call(from_page("GET", path)).await;
        answer.refused(StatusCode::NOT_FOUND, "outside_prefix");
    }
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_read_under_a_write_only_prefix_is_refused() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("GET", "/p/checklist/actions/run"))
        .await;
    answer.refused(StatusCode::NOT_FOUND, "outside_prefix");
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_write_under_a_read_only_prefix_is_refused() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("POST", "/p/checklist/reports/weekly"))
        .await;
    answer.refused(StatusCode::NOT_FOUND, "outside_prefix");
    assert!(shell.seen().is_empty());
}

/// An escape that decodes to an ordinary character names the same path, and
/// is sent in the one spelling the platform will bind it to.
#[tokio::test]
async fn a_harmlessly_encoded_path_is_carried_in_one_spelling() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("POST", "/p/checklist/api/%69tems/a%20b"))
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    let seen = shell.only_seen();
    assert_eq!(seen.path, "/checklist/api/items/a%20b");
    let token = seen.header(&IDENTITY_HEADER.to_lowercase()).unwrap();
    let claims = shell
        .verifier()
        .verify_request(token, "checklist", "POST", "/api/items/a%20b")
        .expect("the platform accepts the path it received");
    assert_eq!(claims.htu(), Some("/api/items/a b"));
}

// -- 4. The method -----------------------------------------------------------

#[tokio::test]
async fn a_method_that_is_neither_read_nor_write_is_refused() {
    let shell = shell().await;
    for method in ["OPTIONS", "TRACE", "PROPFIND"] {
        let answer = shell
            .call(from_page(method, "/p/checklist/api/items"))
            .await;
        answer.refused(StatusCode::METHOD_NOT_ALLOWED, "method");
    }
    assert!(shell.seen().is_empty());
}

/// The path first, so an undeclared path says so whatever it was asked with.
#[tokio::test]
async fn an_odd_method_on_an_undeclared_path_is_outside_the_prefix() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("OPTIONS", "/p/checklist/admin/x"))
        .await;
    answer.refused(StatusCode::NOT_FOUND, "outside_prefix");
}

// -- 5. Writes ---------------------------------------------------------------

#[tokio::test]
async fn a_read_only_shell_refuses_every_write() {
    let shell = read_only_shell().await;
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let answer = shell
            .call(
                Request::builder()
                    .method(method)
                    .uri("/p/checklist/api/items")
                    .header("sec-fetch-site", "same-origin")
                    .header("origin", ORIGIN)
                    .header("idempotency-key", "k"),
            )
            .await;
        answer.refused(StatusCode::FORBIDDEN, "read_only");
    }
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_read_only_shell_still_carries_reads() {
    let shell = read_only_shell().await;
    let answer = shell
        .call(
            Request::builder()
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "same-origin"),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    let seen = shell.only_seen();
    let token = seen.header(&IDENTITY_HEADER.to_lowercase()).unwrap();
    let claims = shell.verifier().verify(token, "checklist").unwrap();
    assert!(!claims.sub.is_empty(), "the visitor is somebody");
}

/// Read-only is decided before idempotency, as the specification orders them.
#[tokio::test]
async fn a_read_only_shell_says_read_only_even_without_a_key() {
    let shell = read_only_shell().await;
    let answer = shell
        .call(
            Request::builder()
                .method("POST")
                .uri("/p/checklist/api/items")
                .header("sec-fetch-site", "same-origin")
                .header("origin", ORIGIN),
        )
        .await;
    answer.refused(StatusCode::FORBIDDEN, "read_only");
}

#[tokio::test]
async fn a_platform_reached_as_everyone_cannot_be_written_to() {
    let shell = shell().await;
    for platform in ["shared", "open"] {
        let answer = shell
            .call(from_page("POST", &format!("/p/{platform}/api/items")))
            .await;
        answer.refused(StatusCode::CONFLICT, "no_identity");
    }
    assert!(shell.seen().is_empty());
}

#[tokio::test]
async fn a_platform_reached_as_everyone_can_still_be_read() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/shared/api/items")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        shell.only_seen().header("authorization"),
        Some("Bearer a-shared-key")
    );
}

#[tokio::test]
async fn a_write_without_an_idempotency_key_is_refused() {
    let shell = shell().await;
    for key in [None, Some(""), Some("   ")] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/p/checklist/api/items")
            .header("sec-fetch-site", "same-origin")
            .header("origin", ORIGIN)
            .header(USER, "alice");
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        let answer = shell.call(request).await;
        answer.refused(StatusCode::BAD_REQUEST, "no_idempotency_key");
    }
    assert!(shell.seen().is_empty());
}

// -- 6. Identity -------------------------------------------------------------

#[tokio::test]
async fn every_write_carries_a_token_bound_to_it() {
    let shell = shell().await;
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let answer = shell
            .call(from_page(method, "/p/checklist/api/lists/team/items/i1"))
            .await;
        assert_eq!(answer.status, StatusCode::OK, "{method}");
    }

    let seen = shell.seen();
    assert_eq!(seen.len(), 4);
    let verifier = shell.verifier();
    for request in seen {
        let token = request.header(&IDENTITY_HEADER.to_lowercase()).unwrap();
        let path = request.path.strip_prefix("/checklist").unwrap();
        let claims = verifier
            .verify_request(token, "checklist", &request.method, path)
            .expect("the platform accepts the token for this request");
        assert_eq!(claims.sub, "alice");
        assert_eq!(claims.htm(), Some(request.method.as_str()));
        assert_eq!(claims.htu(), Some("/api/lists/team/items/i1"));
        assert!(claims.exp - claims.iat <= hlin_identity::BOUND_TOKEN_LIFETIME_SECONDS);

        // And for nothing else.
        assert!(
            verifier
                .verify_request(
                    token,
                    "checklist",
                    &request.method,
                    "/api/lists/team/items/i2"
                )
                .is_err()
        );
        let other = if request.method == "POST" {
            "DELETE"
        } else {
            "POST"
        };
        assert!(
            verifier
                .verify_request(token, "checklist", other, path)
                .is_err()
        );
        assert!(
            verifier
                .verify_request(token, "open", &request.method, path)
                .is_err()
        );
    }
}

#[tokio::test]
async fn a_read_carries_an_unbound_token_and_its_query() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("GET", "/p/checklist/api/items?list=team&page=2"))
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    let seen = shell.only_seen();
    assert_eq!(seen.method, "GET");
    assert_eq!(seen.path, "/checklist/api/items");
    assert_eq!(seen.query.as_deref(), Some("list=team&page=2"));
    assert_eq!(seen.header("idempotency-key"), None);
    let token = seen.header(&IDENTITY_HEADER.to_lowercase()).unwrap();
    let claims = shell
        .verifier()
        .verify_request(token, "checklist", "GET", "/api/items")
        .unwrap();
    assert!(!claims.is_bound());
}

#[tokio::test]
async fn a_head_is_a_read() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("HEAD", "/p/checklist/api/items"))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(shell.only_seen().method, "HEAD");
}

/// A platform sharing the shell's session judges a write by that session, as
/// it would one from its own pages; there is no token to bind.
#[tokio::test]
async fn a_platform_sharing_the_session_is_written_to_with_its_own_cookie() {
    let shell = shell().await;
    let answer = shell
        .call(
            from_page("POST", "/p/session/api/items")
                .header("cookie", "platform_session=abc; hlin_session=shell-only"),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    let seen = shell.only_seen();
    assert_eq!(seen.header("cookie"), Some("platform_session=abc"));
    assert_eq!(seen.header(&IDENTITY_HEADER.to_lowercase()), None);
}

#[tokio::test]
async fn a_viewer_with_no_identity_for_the_platform_is_refused() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/session/api/items")).await;
    answer.refused(StatusCode::CONFLICT, "no_identity");
    assert!(shell.seen().is_empty());
}

// -- 7. The call -------------------------------------------------------------

#[tokio::test]
async fn only_the_allowed_request_headers_reach_the_platform() {
    let shell = shell().await;
    let answer = shell
        .send(
            from_page("PUT", "/p/checklist/api/items/i1")
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .header("if-match", "\"v1\"")
                .header("if-none-match", "\"v0\"")
                .header("cookie", "hlin_session=shell-only")
                .header("authorization", "Bearer from-the-page")
                .header("x-hlin-instance", "7f3c")
                .header("x-forwarded-for", "10.0.0.1")
                .header("x-custom", "anything"),
            r#"{"done":true}"#,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    let seen = shell.only_seen();
    assert_eq!(seen.header("content-type"), Some("application/json"));
    assert_eq!(seen.header("accept"), Some("application/json"));
    assert_eq!(seen.header("if-match"), Some("\"v1\""));
    assert_eq!(seen.header("if-none-match"), Some("\"v0\""));
    assert_eq!(seen.header("idempotency-key"), Some("01J8Z-attempt-1"));
    assert!(seen.header(&IDENTITY_HEADER.to_lowercase()).is_some());
    for absent in [
        "cookie",
        "authorization",
        "x-hlin-instance",
        "x-forwarded-for",
        "x-custom",
        "origin",
        "sec-fetch-site",
        USER,
    ] {
        assert_eq!(seen.header(absent), None, "{absent} crossed");
    }
    assert_eq!(seen.body, br#"{"done":true}"#);
}

#[tokio::test]
async fn a_request_body_over_its_limit_is_refused_before_the_platform_sees_it() {
    let shell = shell().await;
    let answer = shell
        .send(
            from_page("POST", "/p/small/api/items"),
            "x".repeat(SMALL + 1),
        )
        .await;
    answer.refused(StatusCode::PAYLOAD_TOO_LARGE, "too_large");

    // Undeclared length, counted as it arrives.
    let chunks = (0..4).map(|_| Ok::<_, std::convert::Infallible>("x".repeat(SMALL / 2)));
    let answer = shell
        .send(
            from_page("POST", "/p/small/api/items"),
            Body::from_stream(futures::stream::iter(chunks.collect::<Vec<_>>())),
        )
        .await;
    answer.refused(StatusCode::PAYLOAD_TOO_LARGE, "too_large");
    assert!(shell.seen().is_empty());

    let answer = shell
        .send(from_page("POST", "/p/small/api/items"), "x".repeat(SMALL))
        .await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "a body at the limit is carried"
    );
}

#[tokio::test]
async fn a_response_over_its_limit_is_refused() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/small/api/big")).await;
    answer.refused(StatusCode::PAYLOAD_TOO_LARGE, "too_large");

    // The same answer is within the default limit.
    let answer = shell.call(from_page("GET", "/p/checklist/api/big")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body.len(), SMALL * 2);
}

#[tokio::test]
async fn a_platform_nobody_can_reach_is_unreachable() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/gone/api/items")).await;
    answer.refused(StatusCode::BAD_GATEWAY, "unreachable");
}

#[tokio::test]
async fn a_platform_that_does_not_answer_in_time_is_a_timeout() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/checklist/api/slow")).await;
    answer.refused(StatusCode::GATEWAY_TIMEOUT, "timeout");
}

#[tokio::test]
async fn a_failing_write_is_sent_once() {
    let shell = shell().await;

    let answer = shell
        .call(from_page("POST", "/p/checklist/api/broken"))
        .await;
    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);

    let answer = shell.call(from_page("POST", "/p/checklist/api/slow")).await;
    answer.refused(StatusCode::GATEWAY_TIMEOUT, "timeout");

    // Long enough for a retry to have arrived, had there been one.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let seen = shell.seen();
    assert_eq!(
        seen.len(),
        2,
        "each write reached the platform once: {seen:#?}"
    );
}

// -- 8. The answer -----------------------------------------------------------

/// A redirect followed would carry the viewer's identity, bound or not, to an
/// address no prefix was checked against. The platform's redirect is its
/// answer instead, without the address, so the page's own `fetch` has nowhere
/// to follow it either.
#[tokio::test]
async fn a_platforms_redirect_is_its_answer_and_is_never_followed() {
    let shell = shell().await;
    for method in ["GET", "POST"] {
        let answer = shell
            .call(from_page(method, "/p/checklist/api/redirect"))
            .await;
        assert_eq!(answer.status, StatusCode::TEMPORARY_REDIRECT, "{method}");
        assert_eq!(answer.refusal(), None);
        assert_eq!(answer.headers.get("location"), None);
        assert_eq!(answer.headers.get("cache-control").unwrap(), "no-store");
        assert_eq!(answer.body, b"moved");
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(shell.seen().len(), 2);
    let elsewhere = shell.elsewhere.lock().unwrap().clone();
    assert!(
        elsewhere.is_empty(),
        "the redirect was followed: {elsewhere:#?}"
    );
}

#[tokio::test]
async fn only_the_allowed_response_headers_reach_the_module() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/checklist/api/items")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body, br#"{"ok":true}"#);

    let header = |name: &str| answer.headers.get(name).and_then(|v| v.to_str().ok());
    assert_eq!(header("content-type"), Some("application/json"));
    assert_eq!(header("etag"), Some("\"v1\""));
    assert_eq!(
        header("last-modified"),
        Some("Wed, 23 Sep 2026 10:00:00 GMT")
    );
    assert_eq!(header("cache-control"), Some("no-store"));
    for absent in ["set-cookie", "x-internal", "location", "x-hlin-refusal"] {
        assert_eq!(header(absent), None, "{absent} crossed");
    }
}

#[tokio::test]
async fn a_platforms_refusal_is_passed_back_in_its_own_words() {
    let shell = shell().await;
    let answer = shell
        .call(from_page("POST", "/p/checklist/api/items/forbidden"))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.refusal(), None);
    assert_eq!(answer.body, b"Only Alice may edit Alice's items");
    assert_eq!(answer.headers.get("content-type").unwrap(), "text/plain");

    let answer = shell
        .call(from_page("GET", "/p/checklist/api/broken"))
        .await;
    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(answer.refusal(), None);
    assert_eq!(answer.body, b"it broke");
}

/// A 401 from a platform means the shell's credential was refused, which the
/// operator must hear about. The module still gets the platform's answer.
#[tokio::test]
async fn a_platform_refusing_the_shells_credential_is_told_to_the_operator() {
    let logged = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = logged.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || Capture(writer.clone()))
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let shell = shell().await;
    let answer = shell
        .call(from_page("GET", "/p/checklist/api/refused"))
        .await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(answer.refusal(), None);
    assert_eq!(answer.body, b"who are you");

    let logged = String::from_utf8(logged.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains("WARN") && logged.contains("checklist") && logged.contains("credential"),
        "{logged}"
    );
}

struct Capture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every refusal the shell writes itself says so, so the page can tell it from
/// the platform's.
#[tokio::test]
async fn a_shell_refusal_says_whose_it_is_in_its_body_too() {
    let shell = shell().await;
    let answer = shell.call(from_page("GET", "/p/checklist/admin")).await;
    let body: serde_json::Value = serde_json::from_slice(&answer.body).unwrap();
    assert_eq!(body["refusal"], "outside_prefix");
    assert!(
        body["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
}
