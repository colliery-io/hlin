//! The operator's brand, over the real router (COLLIERY-I-0609).
//!
//! The claims:
//!
//! - With no `[brand]`, the shell is Hlin. The stylesheet route answers with
//!   an empty sheet, so a front end can link it always. The logo and the
//!   favicon are 404, and the favicon is not the frontend's `index.html`.
//! - With a `[brand]`, each file is served as its type, and `/api/config`
//!   names the brand.
//! - None of it needs a signed-in caller, because a page has to look like
//!   itself before anybody signs in.
//! - A file replaced on disk is served without a restart.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hlin::brand::BrandConfig;
use hlin::config::{AuthConfig, Config, Timings};
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use tower::ServiceExt;

struct Nowhere;

#[async_trait]
impl ManifestClient for Nowhere {
    async fn fetch(&self, _base_url: &str) -> Fetched {
        Fetched::Unreachable {
            reason: "no platforms in this test".to_string(),
        }
    }
}

/// A shell that signs people in with OIDC would redirect them first; one
/// behind a proxy refuses a request with no principal header. That is the
/// strategy here, so an unauthenticated request is refused by everything that
/// asks who is calling.
fn shell_config(brand: BrandConfig) -> Config {
    Config {
        public_url: None,
        modules: Default::default(),
        compression: Default::default(),
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        brand,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::TrustedHeader {
            header: "x-user".to_string(),
            groups_header: None,
            name_header: None,
            acknowledge_proxy_required: true,
        },
        timings: Timings::default(),
        platforms: vec![],
    }
}

async fn shell(brand: BrandConfig) -> axum::Router {
    let config = Arc::new(shell_config(brand));
    config.check().expect("the configuration is usable");
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Nowhere))
            .expect("registry builds"),
    );
    router(AppState {
        config,
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store,
        client: reqwest::Client::new(),
        stream_client: reqwest::Client::new(),
        proxy_client: hlin::clients::Clients::plain().proxying,
        compressed: Default::default(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    })
}

/// What came back: status, content type, body.
async fn get(app: &axum::Router, path: &str, user: Option<&str>) -> (StatusCode, String, Vec<u8>) {
    let mut request = Request::builder().uri(path);
    if let Some(user) = user {
        request = request.header("x-user", user);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body")
        .to_vec();
    (status, content_type, body)
}

fn brand_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hlin-brand-routes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    dir
}

#[tokio::test]
async fn with_no_brand_the_shell_is_hlin_and_the_sheet_is_empty() {
    let app = shell(BrandConfig::default()).await;

    let (status, content_type, body) = get(&app, "/brand/style.css", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/css"));
    assert!(body.is_empty());

    assert_eq!(
        get(&app, "/brand/logo", None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/favicon.ico", None).await.0,
        StatusCode::NOT_FOUND
    );

    let (status, _, body) = get(&app, "/api/config", Some("alice")).await;
    assert_eq!(status, StatusCode::OK);
    let config: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(config["brand"]["name"], "Hlin");
    assert_eq!(config["brand"]["logo"], false);
}

#[tokio::test]
async fn a_brand_is_served_to_anybody_and_named_in_the_config() {
    let dir = brand_dir();
    let logo = dir.join("logo.svg");
    let favicon = dir.join("favicon.png");
    let sheet = dir.join("brand.css");
    std::fs::write(&logo, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    std::fs::write(&favicon, [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::write(&sheet, ":root { --hlin-accent: #c2185b; }").unwrap();

    let app = shell(BrandConfig {
        name: Some("Acme Work".to_string()),
        logo: Some(logo),
        favicon: Some(favicon),
        stylesheet: Some(sheet.clone()),
    })
    .await;

    // Nobody signed in: the brand is still there.
    let (status, content_type, body) = get(&app, "/brand/style.css", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/css"));
    assert_eq!(body, b":root { --hlin-accent: #c2185b; }");

    let (status, content_type, _) = get(&app, "/brand/logo", None).await;
    assert_eq!(
        (status, content_type.as_str()),
        (StatusCode::OK, "image/svg+xml")
    );

    let (status, content_type, _) = get(&app, "/favicon.ico", None).await;
    assert_eq!(
        (status, content_type.as_str()),
        (StatusCode::OK, "image/png")
    );

    // While the shell's own API still wants to know who is asking.
    assert_eq!(
        get(&app, "/api/config", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, _, body) = get(&app, "/api/config", Some("alice")).await;
    let config: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(config["brand"]["name"], "Acme Work");
    assert_eq!(config["brand"]["logo"], true);

    // Replaced on disk, as a ConfigMap is: served as it is now.
    std::fs::write(&sheet, ":root { --hlin-accent: #00897b; }").unwrap();
    let (_, _, body) = get(&app, "/brand/style.css", None).await;
    assert_eq!(body, b":root { --hlin-accent: #00897b; }");
}

#[tokio::test]
async fn an_svg_opened_directly_cannot_run_script_on_the_shells_origin() {
    let dir = brand_dir();
    let logo = dir.join("scripted.svg");
    std::fs::write(&logo, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    let app = shell(BrandConfig {
        logo: Some(logo),
        ..BrandConfig::default()
    })
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/brand/logo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp = response
        .headers()
        .get("content-security-policy")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(csp.starts_with("default-src 'none'"), "{csp}");
    assert!(!csp.contains("script-src"), "{csp}");
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
}
