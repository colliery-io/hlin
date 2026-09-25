//! Module assets, served from the shell's origin (HLIN-S-0007, *Assets*, *The
//! module CSP*, *The shell page's CSP*).
//!
//! Over the real router and a real platform on a port, because every claim
//! here is about what crosses a wire: which path the shell agreed to ask for,
//! which headers it sent the platform, and which it sent the browser. The
//! platform records every request it gets, so a refusal can be shown to have
//! been decided before the platform was asked anything.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::IntoResponse;
use hlin::config::{AuthConfig, Config, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::modules::{LimitOverrides, PlatformModules};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use hlin_manifest::Manifest;
use tower::ServiceExt;

const PLATFORM: &str = "checklist";
const SHELL: &str = "https://hlin.example.com";

/// Small limits, so the tests can pass them without sending megabytes.
const ENTRY_BYTES: usize = 1024;
const ASSET_BYTES: usize = 4096;

// -- A platform ---------------------------------------------------------------

/// What the platform was asked: each request's path and headers.
type Asked = Arc<Mutex<Vec<(String, HeaderMap)>>>;

async fn platform() -> (String, Asked) {
    let asked: Asked = Arc::default();
    let recorder = asked.clone();

    let app = axum::Router::new().fallback(move |request: Request<Body>| {
        let recorder = recorder.clone();
        async move {
            let path = request.uri().path().to_string();
            recorder
                .lock()
                .unwrap()
                .push((path.clone(), request.headers().clone()));
            answer(&path, request.headers()).await
        }
    });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (base, asked)
}

async fn answer(path: &str, headers: &HeaderMap) -> axum::response::Response {
    match path {
        // Declared as an entry, and mislabelled and marked immutable by the
        // platform: the shell must correct both.
        "/ui/items/index.html" => (
            [
                (header::CONTENT_TYPE, "text/plain"),
                (header::CACHE_CONTROL, "max-age=31536000, immutable"),
            ],
            "<!doctype html><title>items</title>",
        )
            .into_response(),
        "/ui/items/app.wasm" => (
            [(header::CONTENT_TYPE, "application/octet-stream")],
            vec![0u8, 0x61, 0x73, 0x6d],
        )
            .into_response(),
        "/ui/items/app-4f2a.js" => (
            [
                (header::CONTENT_TYPE, "text/javascript"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            "export default 1;",
        )
            .into_response(),
        "/ui/items/style.css" => (
            [
                (header::CONTENT_TYPE, "text/css"),
                (header::CACHE_CONTROL, "max-age=600"),
            ],
            "body{}",
        )
            .into_response(),
        "/ui/items/tagged.js" => {
            if headers
                .get(header::IF_NONE_MATCH)
                .is_some_and(|tag| tag == "\"v1\"")
            {
                return (StatusCode::NOT_MODIFIED, [(header::ETAG, "\"v1\"")]).into_response();
            }
            (
                [
                    (header::CONTENT_TYPE, "text/javascript"),
                    (header::ETAG, "\"v1\""),
                ],
                "export default 2;",
            )
                .into_response()
        }
        // Declared as the navigation entry's module. Under the asset limit,
        // over the entry limit.
        "/ui/nav/index.html" => "x".repeat(ENTRY_BYTES + 1).into_response(),
        "/ui/items/huge.bin" => vec![0u8; ASSET_BYTES + 1].into_response(),
        "/ui/items/fits.bin" => vec![0u8; ASSET_BYTES].into_response(),
        "/ui/items/streamed.bin" => {
            // No Content-Length, so only the running total can refuse it.
            let chunks = (0..8).map(|_| Ok::<_, std::io::Error>(vec![0u8; ASSET_BYTES / 4]));
            Body::from_stream(futures::stream::iter(chunks)).into_response()
        }
        "/ui/items/broken.js" => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        "/ui/items/forbidden.js" => StatusCode::FORBIDDEN.into_response(),
        "/ui/items/moved.js" => {
            (StatusCode::FOUND, [(header::LOCATION, "/admin/secret")]).into_response()
        }
        "/ui/items/slow.js" => {
            tokio::time::sleep(Duration::from_secs(5)).await;
            "too late".into_response()
        }
        "/uix/app.js" | "/admin/secret" | "/ui/secret" => "not for a module".into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

// -- A shell ------------------------------------------------------------------

struct Declares;

#[async_trait]
impl ManifestClient for Declares {
    async fn fetch(&self, _base_url: &str) -> Fetched {
        Fetched::Document(Box::new(manifest()))
    }
}

fn manifest() -> Manifest {
    hlin_manifest::parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": {{ "id": "{PLATFORM}", "name": "Checklist" }},
          "assets": "/ui/",
          "navigation": [
            {{ "label": "Board", "path": "/board",
               "ui": {{ "entry": "/ui/nav/index.html", "bridge": 1 }} }}
          ],
          "panels": [
            {{ "key": "items", "title": "Items",
               "ui": {{ "entry": "/ui/items/index.html", "bridge": 1 }} }}
          ],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

fn config(base: &str) -> Config {
    Config {
        public_url: Some(format!("{SHELL}/some/path")),
        modules: Default::default(),
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::default(),
        timings: Timings::default(),
        platforms: vec![PlatformConfig {
            modules: PlatformModules {
                limits: LimitOverrides {
                    entry_bytes: Some(ENTRY_BYTES),
                    asset_bytes: Some(ASSET_BYTES),
                    ..Default::default()
                },
            },
            id: PLATFORM.to_string(),
            base_url: base.to_string(),
            // A strategy that would put something on the wire if the shell
            // asked it to, so its absence means the shell did not ask.
            auth: CredentialConfig::HlinToken,
        }],
    }
}

async fn state(base: &str) -> AppState {
    let config = Arc::new(config(base));
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Declares))
            .expect("registry builds"),
    );
    registry.poll_all().await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    let proxy_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    AppState {
        config,
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store,
        client: client.clone(),
        stream_client: client,
        proxy_client,
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    }
}

async fn shell() -> (axum::Router, Asked) {
    let (base, asked) = platform().await;
    (router(state(&base).await), asked)
}

struct Answered {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answered {
    fn header(&self, name: header::HeaderName) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

async fn get(app: &axum::Router, request: Request<Body>) -> Answered {
    let response = app.clone().oneshot(request).await.expect("answers");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1 << 24)
        .await
        .unwrap()
        .to_vec();
    Answered {
        status,
        headers,
        body,
    }
}

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header(header::COOKIE, "hlin_session=the-viewers-session")
        .header(header::AUTHORIZATION, "Bearer the-viewers-token")
        .body(Body::empty())
        .unwrap()
}

async fn fetch(app: &axum::Router, path: &str) -> Answered {
    get(app, request(path)).await
}

fn module_csp() -> String {
    let own = format!("{SHELL}/m/{PLATFORM}/");
    format!(
        "default-src 'none'; \
         script-src {own} 'wasm-unsafe-eval'; \
         style-src {own} 'unsafe-inline'; \
         img-src {own} data: blob:; \
         font-src {own}; \
         connect-src {own}; \
         form-action 'none'; \
         base-uri 'none'; \
         frame-ancestors {SHELL}"
    )
}

// -- What is served -----------------------------------------------------------

#[tokio::test]
async fn a_declared_asset_is_fetched_from_the_platform_and_served() {
    let (app, asked) = shell().await;

    let answered = fetch(&app, "/m/checklist/ui/items/app-4f2a.js").await;

    assert_eq!(answered.status, StatusCode::OK);
    assert_eq!(answered.body, b"export default 1;");
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, "/ui/items/app-4f2a.js");
}

#[tokio::test]
async fn the_shell_asks_as_itself_and_carries_nothing_of_the_viewer() {
    let (app, asked) = shell().await;

    fetch(&app, "/m/checklist/ui/items/app-4f2a.js").await;

    let asked = asked.lock().unwrap();
    let (_, headers) = &asked[0];
    assert!(headers.get(header::COOKIE).is_none(), "no viewer cookie");
    assert!(
        headers.get(header::AUTHORIZATION).is_none(),
        "no viewer authorization"
    );
    assert!(
        headers.get(hlin_identity::IDENTITY_HEADER).is_none(),
        "no token minted for the viewer, though the platform's strategy would mint one"
    );
}

// -- What is refused ----------------------------------------------------------

#[tokio::test]
async fn paths_outside_the_prefix_or_breaking_a_path_rule_are_404_and_never_asked_for() {
    let (app, asked) = shell().await;

    for path in [
        // Outside the prefix, including by string but not by segment.
        "/m/checklist/uix/app.js",
        "/m/checklist/admin/secret",
        "/m/checklist/ui",
        "/m/checklist/ui/",
        "/m/checklist/",
        "/m/checklist",
        "/m/",
        "/m",
        // Walking out of it.
        "/m/checklist/ui/../admin/secret",
        "/m/checklist/ui/items/../../admin/secret",
        "/m/checklist/ui/./secret",
        "/m/checklist/ui//secret",
        // Encoded separators and dots, which decode into the above after a
        // naive check has passed them.
        "/m/checklist/ui/%2e%2e/admin/secret",
        "/m/checklist/ui/%2E%2E/admin/secret",
        "/m/checklist/ui%2F..%2Fadmin/secret",
        "/m/checklist/ui/items%2fapp.wasm",
        "/m/checklist/ui/items%5capp.wasm",
        "/m/checklist/ui/items/%2e%2e%2f%2e%2e%2fadmin/secret",
        // Unknown platforms.
        "/m/stampmill/ui/items/app.wasm",
        "/m/Checklist/ui/items/app.wasm",
    ] {
        let answered = fetch(&app, path).await;
        assert_eq!(answered.status, StatusCode::NOT_FOUND, "{path}");
    }

    assert!(
        asked.lock().unwrap().is_empty(),
        "the platform was asked for {:?}",
        asked
            .lock()
            .unwrap()
            .iter()
            .map(|(p, _)| p)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_platform_without_an_assets_prefix_serves_nothing() {
    struct Undeclared;

    #[async_trait]
    impl ManifestClient for Undeclared {
        async fn fetch(&self, _base_url: &str) -> Fetched {
            let mut manifest = manifest();
            manifest.assets = None;
            manifest.panels.clear();
            manifest.navigation.clear();
            Fetched::Document(Box::new(manifest))
        }
    }

    let (base, asked) = platform().await;
    let mut state = state(&base).await;
    let registry = Registry::new(
        &state.config,
        state.issuer.clone(),
        state.store.clone(),
        Arc::new(Undeclared),
    )
    .unwrap();
    registry.poll_all().await;
    state.registry = Arc::new(registry);
    let app = router(state);

    let answered = fetch(&app, "/m/checklist/ui/items/app.wasm").await;
    assert_eq!(answered.status, StatusCode::NOT_FOUND);
    assert!(asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn only_get_is_served() {
    let (app, asked) = shell().await;

    let answered = get(
        &app,
        Request::builder()
            .method("POST")
            .uri("/m/checklist/ui/items/app.wasm")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(answered.status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(asked.lock().unwrap().is_empty());
}

// -- Limits -------------------------------------------------------------------

#[tokio::test]
async fn an_asset_is_bounded_by_asset_bytes() {
    let (app, _) = shell().await;

    let fits = fetch(&app, "/m/checklist/ui/items/fits.bin").await;
    assert_eq!(fits.status, StatusCode::OK);
    assert_eq!(fits.body.len(), ASSET_BYTES);

    let huge = fetch(&app, "/m/checklist/ui/items/huge.bin").await;
    assert_eq!(huge.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(huge.body.len() < 100, "none of the asset is passed on");
}

#[tokio::test]
async fn an_asset_that_does_not_say_how_long_it_is_is_bounded_as_it_arrives() {
    let (app, _) = shell().await;

    let answered = fetch(&app, "/m/checklist/ui/items/streamed.bin").await;

    assert_eq!(answered.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn an_entry_is_bounded_by_entry_bytes() {
    let (app, _) = shell().await;

    // Larger than `entry_bytes` and smaller than `asset_bytes`: refused
    // because it is declared as an entry.
    let answered = fetch(&app, "/m/checklist/ui/nav/index.html").await;

    assert_eq!(answered.status, StatusCode::PAYLOAD_TOO_LARGE);
}

// -- What the frame host can tell apart ---------------------------------------

#[tokio::test]
async fn a_platform_failing_is_a_5xx_and_a_platform_refusing_is_a_4xx() {
    let (app, _) = shell().await;

    let broken = fetch(&app, "/m/checklist/ui/items/broken.js").await;
    assert_eq!(broken.status, StatusCode::BAD_GATEWAY);

    let forbidden = fetch(&app, "/m/checklist/ui/items/forbidden.js").await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let missing = fetch(&app, "/m/checklist/ui/items/missing.js").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_platform_that_does_not_answer_in_time_is_a_gateway_timeout() {
    let (app, _) = shell().await;

    let answered = fetch(&app, "/m/checklist/ui/items/slow.js").await;

    assert_eq!(answered.status, StatusCode::GATEWAY_TIMEOUT);
}

#[tokio::test]
async fn a_platform_that_cannot_be_reached_is_a_bad_gateway() {
    // Bound and dropped, so nothing is listening on it.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = router(state(&base).await);

    let answered = fetch(&app, "/m/checklist/ui/items/app.wasm").await;

    assert_eq!(answered.status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn a_redirect_out_of_the_prefix_is_not_served_or_followed() {
    let (app, asked) = shell().await;

    let answered = fetch(&app, "/m/checklist/ui/items/moved.js").await;

    assert_eq!(answered.status, StatusCode::BAD_GATEWAY);
    assert_ne!(answered.body, b"not for a module");
    let asked = asked.lock().unwrap();
    assert!(
        asked.iter().all(|(path, _)| path != "/admin/secret"),
        "the address a redirect names is never asked for: {:?}",
        asked.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
}

// -- Types --------------------------------------------------------------------

#[tokio::test]
async fn wasm_and_html_are_typed_by_the_shell_and_everything_else_by_the_platform() {
    let (app, _) = shell().await;

    let wasm = fetch(&app, "/m/checklist/ui/items/app.wasm").await;
    assert_eq!(wasm.header(header::CONTENT_TYPE), Some("application/wasm"));

    let html = fetch(&app, "/m/checklist/ui/items/index.html").await;
    assert_eq!(
        html.header(header::CONTENT_TYPE),
        Some("text/html; charset=utf-8")
    );

    let css = fetch(&app, "/m/checklist/ui/items/style.css").await;
    assert_eq!(css.header(header::CONTENT_TYPE), Some("text/css"));

    let js = fetch(&app, "/m/checklist/ui/items/app-4f2a.js").await;
    assert_eq!(js.header(header::CONTENT_TYPE), Some("text/javascript"));
    assert_eq!(js.header(header::X_CONTENT_TYPE_OPTIONS), Some("nosniff"));
}

// -- Caching ------------------------------------------------------------------

#[tokio::test]
async fn the_entry_is_revalidated_even_when_the_platform_calls_it_immutable() {
    let (app, _) = shell().await;

    let entry = fetch(&app, "/m/checklist/ui/items/index.html").await;

    assert_eq!(entry.header(header::CACHE_CONTROL), Some("no-cache"));
}

#[tokio::test]
async fn an_asset_the_platform_calls_immutable_is_cached_as_it_said() {
    let (app, _) = shell().await;

    let hashed = fetch(&app, "/m/checklist/ui/items/app-4f2a.js").await;

    assert_eq!(
        hashed.header(header::CACHE_CONTROL),
        Some("public, max-age=31536000, immutable")
    );
}

#[tokio::test]
async fn any_other_asset_is_revalidated() {
    let (app, _) = shell().await;

    let styled = fetch(&app, "/m/checklist/ui/items/style.css").await;
    assert_eq!(styled.header(header::CACHE_CONTROL), Some("no-cache"));

    let untold = fetch(&app, "/m/checklist/ui/items/app.wasm").await;
    assert_eq!(untold.header(header::CACHE_CONTROL), Some("no-cache"));
}

#[tokio::test]
async fn revalidating_an_unchanged_asset_costs_a_304_not_the_asset() {
    let (app, asked) = shell().await;

    let first = fetch(&app, "/m/checklist/ui/items/tagged.js").await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.header(header::ETAG), Some("\"v1\""));

    let again = get(
        &app,
        Request::builder()
            .uri("/m/checklist/ui/items/tagged.js")
            .header(header::IF_NONE_MATCH, "\"v1\"")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    assert_eq!(again.header(header::CACHE_CONTROL), Some("no-cache"));
    assert_eq!(
        again.header(header::CONTENT_SECURITY_POLICY),
        Some(module_csp().as_str())
    );

    let asked = asked.lock().unwrap();
    assert_eq!(
        asked[1].1.get(header::IF_NONE_MATCH).unwrap(),
        "\"v1\"",
        "the browser's validator reached the platform"
    );
}

// -- The module CSP -----------------------------------------------------------

#[tokio::test]
async fn every_answer_under_m_carries_the_module_csp_with_the_shells_origin() {
    let (app, _) = shell().await;

    for path in [
        "/m/checklist/ui/items/index.html",
        "/m/checklist/ui/items/app.wasm",
        "/m/checklist/ui/items/huge.bin",
        "/m/checklist/ui/items/broken.js",
        "/m/checklist/ui/items/missing.js",
        "/m/checklist/ui/../admin/secret",
        "/m/checklist/uix/app.js",
    ] {
        let answered = fetch(&app, path).await;
        assert_eq!(
            answered.header(header::CONTENT_SECURITY_POLICY),
            Some(module_csp().as_str()),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_frame_whose_origin_is_opaque_may_read_its_own_assets() {
    // A sandboxed frame's origin is `null`, so its module script and the fetch
    // of its `.wasm` are cross-origin CORS requests, refused without this.
    let (app, _) = shell().await;

    for path in [
        "/m/checklist/ui/items/index.html",
        "/m/checklist/ui/items/app.wasm",
    ] {
        let answered = get(
            &app,
            Request::get(path)
                .header(header::ORIGIN, "null")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(answered.status, StatusCode::OK, "{path}");
        assert_eq!(
            answered.header(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some("*"),
            "{path}"
        );
        assert_eq!(
            answered.header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            None,
            "nothing about a session is ever shared: {path}"
        );
    }
}

#[tokio::test]
async fn an_unknown_platform_is_refused_under_a_policy_that_allows_nothing() {
    let (app, _) = shell().await;

    for path in [
        "/m/stampmill/ui/items/app.wasm",
        "/m/x;script-src%20*/ui/app.js",
        "/m",
    ] {
        let answered = fetch(&app, path).await;
        assert_eq!(answered.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            answered.header(header::CONTENT_SECURITY_POLICY),
            Some(format!("default-src 'none'; frame-ancestors {SHELL}").as_str()),
            "{path}"
        );
    }
}

// -- The shell page's CSP -----------------------------------------------------

fn bundle() -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "hlin-assets-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("index.html"),
        "<!doctype html><title>hlin</title>",
    )
    .unwrap();
    std::fs::write(directory.join("app.js"), "console.log(1)").unwrap();
    directory
}

#[tokio::test]
async fn the_shell_page_may_frame_only_module_assets() {
    let (base, _) = platform().await;
    let state = state(&base).await;
    let config = state.config.clone();
    let directory = bundle();
    let app = hlin::server::with_frontend(router(state), &directory, &config);

    let page_csp = format!("frame-src {SHELL}/m/");
    for path in ["/", "/index.html", "/app.js", "/layouts/some-deep-link"] {
        let answered = fetch(&app, path).await;
        assert_eq!(answered.status, StatusCode::OK, "{path}");
        assert_eq!(
            answered.header(header::CONTENT_SECURITY_POLICY),
            Some(page_csp.as_str()),
            "{path}"
        );
    }

    // The API is not a page, and a module's assets carry their own policy.
    let api = fetch(&app, "/api/health").await;
    assert!(api.header(header::CONTENT_SECURITY_POLICY).is_none());
    let module = fetch(&app, "/m/checklist/ui/items/app.wasm").await;
    assert_eq!(
        module.header(header::CONTENT_SECURITY_POLICY),
        Some(module_csp().as_str())
    );

    std::fs::remove_dir_all(directory).ok();
}

#[tokio::test]
async fn a_link_to_a_platform_page_loads_the_shell_page() {
    let (base, _) = platform().await;
    let state = state(&base).await;
    let config = state.config.clone();
    let directory = bundle();
    let app = hlin::server::with_frontend(router(state), &directory, &config);

    // A page's address is the shell's own, beside `/s/`, and nothing the
    // request proxy (`/p/`) or the module assets (`/m/`) claim: sent to
    // somebody, or reloaded, it loads the frontend, which opens the page.
    let page_csp = format!("frame-src {SHELL}/m/");
    for path in ["/page/checklist/lists", "/page/checklist/lists/nested"] {
        let answered = fetch(&app, path).await;
        assert_eq!(answered.status, StatusCode::OK, "{path}");
        assert_eq!(
            answered.header(header::CONTENT_SECURITY_POLICY),
            Some(page_csp.as_str()),
            "{path}"
        );
        assert!(
            String::from_utf8_lossy(&answered.body).contains("<title>hlin</title>"),
            "{path} is answered with the frontend"
        );
    }

    std::fs::remove_dir_all(directory).ok();
}
