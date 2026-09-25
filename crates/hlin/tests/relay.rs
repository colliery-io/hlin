//! A platform's modules hearing that it changed, on every surface the shell
//! serves (specification HLIN-S-0007, `changed`; HLIN-S-0003, the `changed`
//! frame).
//!
//! Two roads bring the news, and both end at the same frame. A platform's own
//! event stream reaches the surfaces that follow it; a module's word that it
//! wrote reaches only the page that framed it, which posts it to the shell so
//! that everyone else's surface hears it too. These drive the real router and
//! real running surfaces, with a real socket for the platform's stream, because
//! what is claimed is that a frame arrives in a browser's stream and only in
//! the right ones.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hlin::config::{AuthConfig, Config, CookieConfig, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use hlin_stream::layout::{LayoutDocument, PanelInstanceDocument, Placement};
use hlin_stream::{ChangeOrigin, ChangedFrame, Frame};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;

/// Ships modules, and reports changes on an event stream.
const MODULAR: &str = "checklist";
/// Ships no modules: only panels the shell draws.
const PLAIN: &str = "orebank";

struct Manifests {
    /// Where `checklist` is, so its manifest can name its own event stream.
    modular_base: String,
}

#[async_trait]
impl ManifestClient for Manifests {
    async fn fetch(&self, base_url: &str) -> Fetched {
        let manifest = if base_url == self.modular_base {
            r#"{
              "schema_version": 1, "contract_version": "1.0.0",
              "platform": { "id": "checklist", "name": "Checklist" },
              "assets": "/ui/", "routes": { "read": ["/api/"], "write": ["/api/"] },
              "events": "api/events",
              "panels": [
                { "key": "items", "title": "To do",
                  "ui": { "entry": "/ui/items/index.html", "bridge": 1 } }
              ],
              "health": "api/health"
            }"#
        } else {
            r#"{
              "schema_version": 1, "contract_version": "1.0.0",
              "platform": { "id": "orebank", "name": "Orebank" },
              "panels": [
                { "key": "throughput", "title": "Throughput", "kind": "stat",
                  "envelope": "scalar.v1", "data": "api/throughput" }
              ],
              "health": "api/health"
            }"#
        };
        Fetched::Document(Box::new(
            hlin_manifest::parse_str(manifest).expect("fixture parses"),
        ))
    }
}

/// A platform whose event stream says `items` changed a moment after anyone
/// subscribes, then holds the connection open. Every connection gets the same.
async fn platform_reporting() -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut scratch = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut scratch).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                          transfer-encoding: chunked\r\n\r\n",
                    )
                    .await;
                tokio::time::sleep(Duration::from_millis(300)).await;
                let event = "event: changed\ndata: {\"panel\":\"items\"}\n\n";
                let chunk = format!("{:x}\r\n{event}\r\n", event.len());
                let _ = socket.write_all(chunk.as_bytes()).await;
                let _ = socket.flush().await;
                std::future::pending::<()>().await;
            });
        }
    });

    base
}

fn config(modular_base: &str) -> Config {
    Config {
        public_url: None,
        modules: Default::default(),
        compression: Default::default(),
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::default(),
        timings: Timings {
            poll_seconds: 30,
            debounce_observations: 2,
            upstream_timeout_seconds: 10,
            ..Default::default()
        },
        platforms: vec![
            PlatformConfig {
                modules: Default::default(),
                id: MODULAR.to_string(),
                base_url: modular_base.to_string(),
                auth: CredentialConfig::HlinToken,
            },
            PlatformConfig {
                modules: Default::default(),
                id: PLAIN.to_string(),
                // Nothing listens here: the shell-drawn panel is never the
                // point, and a refused fetch is all it needs to be.
                base_url: "http://127.0.0.1:9".to_string(),
                auth: CredentialConfig::HlinToken,
            },
        ],
    }
}

struct Shell {
    app: axum::Router,
    state: AppState,
}

async fn shell_with(configured: Config, modular_base: &str) -> Shell {
    let config = Arc::new(configured);
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(
            &config,
            issuer.clone(),
            store.clone(),
            Arc::new(Manifests {
                modular_base: modular_base.to_string(),
            }),
        )
        .expect("registry builds"),
    );
    registry.poll_all().await;

    let state = AppState {
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
    };
    Shell {
        app: router(state.clone()),
        state,
    }
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(request).await.expect("an answer");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a readable body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn json(method: &str, path: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("a request")
}

/// A new layout holding these panels, and its id.
async fn layout(app: &axum::Router, panels: &[(&str, &str)]) -> String {
    let (status, body) = call(
        app,
        json("POST", "/api/layouts", &serde_json::json!({ "title": "t" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let mut document: LayoutDocument = serde_json::from_value(body).expect("a layout");
    let id = document.id.clone().expect("an identity");
    document.panels = panels
        .iter()
        .enumerate()
        .map(|(index, &(platform, key))| {
            PanelInstanceDocument::new(
                platform,
                key,
                Placement {
                    y: index as u32 * 4,
                    ..Placement::default()
                },
            )
        })
        .collect();
    let (status, body) = call(
        app,
        json(
            "PUT",
            &format!("/api/layouts/{id}"),
            &serde_json::to_value(&document).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    id
}

/// What the shell's own page sends: same-origin, and saying so.
fn from_the_page(surface: &str, change: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/stream/{surface}/changed"))
        .header("content-type", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("origin", "http://localhost:8080")
        .body(Body::from(change.to_string()))
        .expect("a request")
}

/// Everything a running surface sends its browsers, from now on.
async fn watching(shell: &Shell, surface: &str) -> tokio::sync::broadcast::Receiver<Frame> {
    let headers = axum::http::HeaderMap::new();
    let principal = shell
        .state
        .config
        .principal_from(&headers)
        .known()
        .expect("the dev authenticator always answers");
    shell
        .state
        .surfaces
        .for_surface(surface, &principal, &shell.state, &headers)
        .await
        .expect("a surface")
        .subscribe()
}

/// The next `changed` frame, if one comes within a couple of seconds.
async fn next_change(
    frames: &mut tokio::sync::broadcast::Receiver<Frame>,
    within: Duration,
) -> Option<ChangedFrame> {
    tokio::time::timeout(within, async {
        loop {
            match frames.recv().await {
                Ok(Frame::Changed(changed)) => return Some(changed),
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

#[tokio::test]
async fn a_modules_change_reaches_every_surface_showing_that_platforms_modules_and_no_other() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;

    let mine = layout(&shell.app, &[(MODULAR, "items")]).await;
    let theirs = layout(&shell.app, &[(MODULAR, "items"), (PLAIN, "throughput")]).await;
    let unrelated = layout(&shell.app, &[(PLAIN, "throughput")]).await;

    let mut on_mine = watching(&shell, &mine).await;
    let mut on_theirs = watching(&shell, &theirs).await;
    let mut on_unrelated = watching(&shell, &unrelated).await;

    // The platform's own event arrives first, on both surfaces showing its
    // module, a moment after they subscribed. Set it aside.
    for frames in [&mut on_mine, &mut on_theirs] {
        let heard = next_change(frames, Duration::from_secs(5))
            .await
            .expect("the platform's own event reaches its modules");
        assert_eq!(heard.from, ChangeOrigin::Platform);
        assert_eq!(heard.page, None);
    }

    let (status, body) = call(
        &shell.app,
        from_the_page(
            &mine,
            &serde_json::json!({
                "platform": MODULAR, "panel": "items",
                "selections": { "list": ["team"] }, "page": "page-a"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    for frames in [&mut on_mine, &mut on_theirs] {
        let heard = next_change(frames, Duration::from_secs(3))
            .await
            .expect("every surface showing the platform's modules hears it");
        assert_eq!(heard.platform, MODULAR);
        assert_eq!(heard.panel, "items");
        assert_eq!(heard.from, ChangeOrigin::Module);
        assert_eq!(
            heard.selections.get("list"),
            Some(&vec!["team".to_string()])
        );
        assert_eq!(
            heard.page.as_deref(),
            Some("page-a"),
            "so the page that relayed it can skip its own echo"
        );
    }

    assert_eq!(
        next_change(&mut on_unrelated, Duration::from_millis(600)).await,
        None,
        "a surface with none of the platform's modules is told nothing"
    );
}

#[tokio::test]
async fn a_platform_with_a_module_on_the_surface_is_followed_though_no_panel_is_pushed() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;
    let surface = layout(&shell.app, &[(MODULAR, "items")]).await;

    let mut frames = watching(&shell, &surface).await;
    let heard = next_change(&mut frames, Duration::from_secs(5))
        .await
        .expect("the module hears its platform through the shell's one subscription");
    assert_eq!(
        (heard.platform.as_str(), heard.panel.as_str(), heard.from),
        (MODULAR, "items", ChangeOrigin::Platform)
    );
}

#[tokio::test]
async fn a_change_that_did_not_come_from_the_shells_page_is_refused() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;
    let surface = layout(&shell.app, &[(MODULAR, "items")]).await;

    // No `Sec-Fetch-Site`: a script elsewhere, or anything that is not a
    // browser running the shell's own page.
    let response = shell
        .app
        .clone()
        .oneshot(json(
            "POST",
            &format!("/api/stream/{surface}/changed"),
            &serde_json::json!({ "platform": MODULAR, "panel": "items" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("x-hlin-refusal").unwrap(),
        "not_from_shell"
    );
}

/// The relay holds the page to the request proxy's origin, so with no
/// `public_url` a page opened at `127.0.0.1` is the shell's as much as one at
/// `localhost`, and neither is the other's.
#[tokio::test]
async fn with_no_public_url_the_page_is_at_whichever_name_it_was_opened_by() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;
    let surface = layout(&shell.app, &[(MODULAR, "items")]).await;
    let change = serde_json::json!({ "platform": MODULAR, "panel": "items" });

    for (host, origin, expected) in [
        (
            "localhost:8080",
            "http://localhost:8080",
            StatusCode::ACCEPTED,
        ),
        (
            "127.0.0.1:8080",
            "http://127.0.0.1:8080",
            StatusCode::ACCEPTED,
        ),
        (
            "127.0.0.1:8080",
            "http://localhost:8080",
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut request = from_the_page(&surface, &change);
        let headers = request.headers_mut();
        headers.insert("host", host.parse().unwrap());
        headers.insert("origin", origin.parse().unwrap());
        let response = shell.app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected, "{host} from {origin}");
    }
}

#[tokio::test]
async fn a_change_for_a_platform_whose_module_is_not_on_the_layout_goes_nowhere() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;
    let surface = layout(&shell.app, &[(PLAIN, "throughput")]).await;
    let mut frames = shell.state.streams.relayed();

    for platform in [MODULAR, PLAIN, "nobody"] {
        let (status, _) = call(
            &shell.app,
            from_the_page(
                &surface,
                &serde_json::json!({ "platform": platform, "panel": "items" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{platform}");
    }
    assert!(frames.try_recv().is_err(), "and nothing was relayed");
}

#[tokio::test]
async fn a_read_only_shell_relays_nothing_because_nothing_was_written() {
    let base = platform_reporting().await;
    let mut configured = config(&base);
    configured.auth = AuthConfig::Anonymous {
        cookie: CookieConfig {
            name: "hlin_visitor".to_string(),
            secure: false,
            same_site: "Lax".to_string(),
        },
        name: None,
    };
    let shell = shell_with(configured, &base).await;

    let response = shell
        .app
        .clone()
        .oneshot(from_the_page(
            "00000000-0000-0000-0000-000000000000",
            &serde_json::json!({ "platform": MODULAR, "panel": "items" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("x-hlin-refusal").unwrap(),
        "read_only"
    );
}

#[tokio::test]
async fn a_change_larger_than_any_honest_one_is_refused_unread() {
    let base = platform_reporting().await;
    let shell = shell_with(config(&base), &base).await;
    let surface = layout(&shell.app, &[(MODULAR, "items")]).await;

    let (status, _) = call(
        &shell.app,
        from_the_page(
            &surface,
            &serde_json::json!({
                "platform": MODULAR, "panel": "items",
                "selections": { "list": ["x".repeat(hlin::modules::changes::MOST_PER_CHANGE)] }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
