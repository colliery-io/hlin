//! What the shell compresses, and what it must not (HLIN-T-0086), and that
//! it compresses each file once (HLIN-T-0091).
//!
//! Module assets under `/m/` and the shell's own frontend are compressed as
//! the browser accepts, unless the platform already did; a module's requests
//! under `/p/` never are, because an answer there may be streamed and each
//! piece must reach the module as it arrives. Over the real router and a real
//! platform on a loopback socket, as the asset and request tests are, because
//! every claim here is about headers on a wire.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::IntoResponse;
use hlin::compressed::{Compressed, CompressionConfig, Held};
use hlin::config::{AuthConfig, Config, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use tower::ServiceExt;

const PLATFORM: &str = "checklist";
const ORIGIN: &str = "https://hlin.example.com";
const USER: &str = "x-forwarded-user";

/// Something that compresses well and is well over the minimum.
fn wasm() -> Vec<u8> {
    let mut body = b"\0asm\x01\0\0\0".to_vec();
    body.extend((0..64 * 1024).map(|i| (i % 7) as u8));
    body
}

fn big_text() -> String {
    "{\"items\":[".to_string() + &"{\"title\":\"a thing to do\"},".repeat(2000) + "{}]}"
}

// -- A platform ---------------------------------------------------------------

type Asked = Arc<Mutex<Vec<(String, HeaderMap)>>>;

/// Which version of `changing.js` the platform serves.
type Version = Arc<Mutex<u32>>;

/// A file that changes under the same name, as a platform's unhashed file
/// does when it is deployed again.
fn changing(version: u32) -> String {
    format!("export const version = {version};\n") + &"// padding\n".repeat(400)
}

async fn platform() -> (String, Asked, Version) {
    let asked: Asked = Arc::default();
    let recorder = asked.clone();
    let version: Version = Arc::new(Mutex::new(1));
    let current = version.clone();

    let app = axum::Router::new().fallback(move |request: Request<Body>| {
        let recorder = recorder.clone();
        let current = current.clone();
        async move {
            let path = request.uri().path().to_string();
            let headers = request.headers().clone();
            recorder
                .lock()
                .unwrap()
                .push((path.clone(), headers.clone()));
            match path.as_str() {
                "/ui/items/index.html" => "<!doctype html><title>items</title>".into_response(),
                "/ui/items/app-0123456789abcdef_bg.wasm" => {
                    if headers
                        .get(header::IF_NONE_MATCH)
                        .is_some_and(|tag| tag == "\"w1\"")
                    {
                        return (StatusCode::NOT_MODIFIED, [(header::ETAG, "\"w1\"")])
                            .into_response();
                    }
                    (
                        [
                            (header::CONTENT_TYPE, "application/wasm"),
                            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                            (header::ETAG, "\"w1\""),
                        ],
                        wasm(),
                    )
                        .into_response()
                }
                "/ui/items/changing.js" => (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    changing(*current.lock().unwrap()),
                )
                    .into_response(),
                "/ui/items/tiny.js" => (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    "export default 1;",
                )
                    .into_response(),
                // A platform that compresses its own files. The bytes are not
                // really gzip: the shell must pass them on untouched, and
                // must not compress them again.
                "/ui/items/packed.js" => (
                    [
                        (header::CONTENT_TYPE, "text/javascript"),
                        (header::CONTENT_ENCODING, "gzip"),
                        (header::VARY, "Accept-Encoding"),
                    ],
                    vec![0x1f, 0x8b]
                        .into_iter()
                        .chain([7u8; 4096])
                        .collect::<Vec<_>>(),
                )
                    .into_response(),
                "/api/big" => {
                    ([(header::CONTENT_TYPE, "application/json")], big_text()).into_response()
                }
                _ => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (base, asked, version)
}

// -- A shell ------------------------------------------------------------------

struct Declares;

#[async_trait]
impl ManifestClient for Declares {
    async fn fetch(&self, _base_url: &str) -> Fetched {
        Fetched::Document(Box::new(
            hlin_manifest::parse_str(&format!(
                r#"{{
                  "schema_version": 1,
                  "contract_version": "1.0.0",
                  "platform": {{ "id": "{PLATFORM}", "name": "Checklist" }},
                  "assets": "/ui/",
                  "panels": [
                    {{ "key": "items", "title": "Items",
                       "ui": {{ "entry": "/ui/items/index.html", "bridge": 1 }} }}
                  ],
                  "routes": {{ "read": ["/api/"] }},
                  "health": "api/health"
                }}"#
            ))
            .expect("fixture parses"),
        ))
    }
}

struct Shell {
    app: axum::Router,
    asked: Asked,
    version: Version,
    compressed: Arc<hlin::compressed::Compressed>,
    frontend: std::path::PathBuf,
}

impl Drop for Shell {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.frontend).ok();
    }
}

async fn shell() -> Shell {
    shell_keeping(CompressionConfig::default()).await
}

async fn shell_keeping(compression: CompressionConfig) -> Shell {
    let (base, asked, version) = platform().await;
    let config = Arc::new(Config {
        public_url: Some(ORIGIN.to_string()),
        modules: Default::default(),
        compression,
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::TrustedHeader {
            header: USER.to_string(),
            groups_header: None,
            name_header: None,
            acknowledge_proxy_required: true,
        },
        timings: Timings::default(),
        platforms: vec![PlatformConfig {
            id: PLATFORM.to_string(),
            base_url: base,
            auth: CredentialConfig::None,
            modules: Default::default(),
        }],
    });

    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Declares))
            .expect("registry builds"),
    );
    registry.poll_all().await;
    let clients = hlin::clients::Clients::build(&config).expect("clients build");

    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store,
        client: clients.fetching,
        stream_client: clients.streaming,
        proxy_client: clients.proxying,
        compressed: Arc::new(Compressed::new(&config.compression)),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    let frontend = std::env::temp_dir().join(format!(
        "hlin-compression-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::write(
        frontend.join("index.html"),
        "<!doctype html><title>hlin</title>",
    )
    .unwrap();
    std::fs::write(frontend.join("hlin-ui_bg.wasm"), wasm()).unwrap();

    Shell {
        app: hlin::server::with_frontend(router(state.clone()), &frontend, &state),
        asked,
        version,
        compressed: state.compressed.clone(),
        frontend,
    }
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

    fn varies_by_encoding(&self) -> bool {
        self.headers.get_all(header::VARY).iter().any(|value| {
            value
                .to_str()
                .unwrap()
                .to_ascii_lowercase()
                .contains("accept-encoding")
        })
    }
}

impl Shell {
    async fn get(&self, path: &str, accepts: Option<&str>) -> Answered {
        self.send(path, accepts, &[]).await
    }

    async fn send(&self, path: &str, accepts: Option<&str>, extra: &[(&str, &str)]) -> Answered {
        self.send_as("alice", path, accepts, extra).await
    }

    /// Until `entries` files are kept, every one at its best: the frontend
    /// is compressed at its best from the start, and a module's file after
    /// its first request has been answered.
    async fn settled(&self, entries: usize) -> Held {
        for _ in 0..1000 {
            let held = self.compressed.held();
            if held.entries >= entries && held.best == held.entries {
                return held;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("never settled: {:?}", self.compressed.held());
    }

    async fn send_as(
        &self,
        who: &str,
        path: &str,
        accepts: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Answered {
        let mut request = Request::builder()
            .uri(path)
            .header("sec-fetch-site", "same-origin")
            .header(USER, who);
        if let Some(accepts) = accepts {
            request = request.header(header::ACCEPT_ENCODING, accepts);
        }
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .expect("answers");
        let status = response.status();
        let headers = response.headers().clone();
        let body = tokio::time::timeout(
            Duration::from_secs(5),
            axum::body::to_bytes(response.into_body(), 1 << 24),
        )
        .await
        .expect("the body ends")
        .unwrap()
        .to_vec();
        Answered {
            status,
            headers,
            body,
        }
    }
}

fn gunzip(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(body)
        .read_to_end(&mut out)
        .expect("gzip");
    out
}

fn unbrotli(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    brotli_decompressor::Decompressor::new(body, 4096)
        .read_to_end(&mut out)
        .expect("brotli");
    out
}

const WASM: &str = "/m/checklist/ui/items/app-0123456789abcdef_bg.wasm";

// -- Module assets --------------------------------------------------------------

#[tokio::test]
async fn a_module_asset_is_compressed_as_the_browser_accepts() {
    let shell = shell().await;

    let gzip = shell.get(WASM, Some("gzip")).await;
    assert_eq!(gzip.status, StatusCode::OK);
    assert_eq!(gzip.header(header::CONTENT_ENCODING), Some("gzip"));
    assert!(gzip.varies_by_encoding());
    assert!(
        gzip.body.len() < wasm().len() / 4,
        "{} bytes",
        gzip.body.len()
    );
    assert_eq!(gunzip(&gzip.body), wasm());

    // What a browser sends: brotli preferred where it is offered.
    let br = shell.get(WASM, Some("gzip, deflate, br, zstd")).await;
    assert_eq!(br.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&br.body), wasm());

    // Everything else the route promises still holds.
    for answered in [&gzip, &br] {
        assert_eq!(
            answered.header(header::CONTENT_TYPE),
            Some("application/wasm")
        );
        assert!(answered.header(header::CONTENT_SECURITY_POLICY).is_some());
        assert_eq!(answered.header(header::ETAG), Some("\"w1\""));
        assert!(
            answered
                .header(header::CACHE_CONTROL)
                .unwrap()
                .contains("immutable")
        );
    }

    // A browser that accepts nothing gets it as it is.
    let plain = shell.get(WASM, None).await;
    assert!(plain.header(header::CONTENT_ENCODING).is_none());
    assert_eq!(plain.body, wasm());
    assert!(
        plain.varies_by_encoding(),
        "a cache must not hand this to a gzip browser as if it were the only form"
    );
}

#[tokio::test]
async fn a_small_asset_is_not_worth_compressing() {
    let shell = shell().await;
    let tiny = shell
        .get("/m/checklist/ui/items/tiny.js", Some("gzip, br"))
        .await;
    assert_eq!(tiny.status, StatusCode::OK);
    assert!(tiny.header(header::CONTENT_ENCODING).is_none());
    assert_eq!(tiny.body, b"export default 1;");
}

#[tokio::test]
async fn what_the_platform_compressed_is_passed_on_as_it_came_and_not_again() {
    let shell = shell().await;
    let packed = shell
        .get("/m/checklist/ui/items/packed.js", Some("gzip, br"))
        .await;
    assert_eq!(packed.status, StatusCode::OK);
    assert_eq!(packed.header(header::CONTENT_ENCODING), Some("gzip"));
    assert!(packed.varies_by_encoding());
    assert_eq!(packed.body.len(), 4098);
    assert_eq!(&packed.body[..2], &[0x1f, 0x8b]);
    assert_eq!(packed.body[2..], [7u8; 4096]);

    // The platform was told what the browser can read, so it could choose.
    let asked = shell.asked.lock().unwrap();
    let (_, headers) = asked
        .iter()
        .find(|(path, _)| path == "/ui/items/packed.js")
        .unwrap();
    assert_eq!(headers[header::ACCEPT_ENCODING], "gzip, br");
}

#[tokio::test]
async fn revalidating_a_compressed_asset_is_still_a_304() {
    let shell = shell().await;
    let again = shell
        .send(WASM, Some("gzip, br"), &[("if-none-match", "\"w1\"")])
        .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    assert!(again.header(header::CONTENT_ENCODING).is_none());
}

// -- The shell's own frontend -------------------------------------------------

#[tokio::test]
async fn the_frontend_is_compressed_and_keeps_its_policy() {
    let shell = shell().await;
    let wasm_answer = shell
        .get("/hlin-ui_bg.wasm", Some("gzip, deflate, br"))
        .await;
    assert_eq!(wasm_answer.status, StatusCode::OK);
    assert_eq!(wasm_answer.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&wasm_answer.body), wasm());
    assert_eq!(
        wasm_answer.header(header::CONTENT_SECURITY_POLICY),
        Some(format!("frame-src {ORIGIN}/m/").as_str())
    );

    let gzip = shell.get("/hlin-ui_bg.wasm", Some("gzip")).await;
    assert_eq!(gzip.header(header::CONTENT_ENCODING), Some("gzip"));
    assert_eq!(gunzip(&gzip.body), wasm());
}

// -- A module's requests ------------------------------------------------------

#[tokio::test]
async fn a_modules_request_is_never_compressed_whole_or_streamed() {
    let shell = shell().await;

    // Large and compressible: exactly what the layer would take, were the
    // route under it.
    let whole = shell.get("/p/checklist/api/big", Some("gzip, br")).await;
    assert_eq!(
        whole.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&whole.body)
    );
    assert!(whole.header(header::CONTENT_ENCODING).is_none());
    assert!(!whole.varies_by_encoding());
    assert_eq!(whole.body, big_text().as_bytes());

    let streamed = shell
        .send(
            "/p/checklist/api/big",
            Some("gzip, br"),
            &[(hlin_stream::streamed::STREAM_HEADER, "1")],
        )
        .await;
    assert_eq!(streamed.status, StatusCode::OK);
    assert!(streamed.header(header::CONTENT_ENCODING).is_none());
    assert!(!streamed.varies_by_encoding());
}

// -- Once per file --------------------------------------------------------------

const BROWSER: Option<&str> = Some("gzip, deflate, br, zstd");

#[tokio::test]
async fn a_file_is_compressed_once_and_kept_for_everybody() {
    let shell = shell().await;
    // The frontend's wasm, compressed at its best since the shell started.
    let frontend = shell.settled(1).await;
    let path = "/m/checklist/ui/items/changing.js";

    let first = shell.send_as("alice", path, BROWSER, &[]).await;
    assert_eq!(first.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&first.body), changing(1).as_bytes());
    assert_eq!(shell.compressed.held().entries, frontend.entries + 1);

    // Compressed again at its best, behind the first answer.
    let settled = shell.settled(frontend.entries + 1).await;

    // Somebody else, later: the same file, one entry, at its best.
    let later = shell.send_as("bob", path, BROWSER, &[]).await;
    assert_eq!(unbrotli(&later.body), changing(1).as_bytes());
    assert!(
        later.body.len() <= first.body.len(),
        "{} > {}",
        later.body.len(),
        first.body.len()
    );
    let again = shell.send_as("carol", path, BROWSER, &[]).await;
    assert_eq!(again.body, later.body, "served from what was kept");
    assert_eq!(shell.compressed.held(), settled);

    // gzip is its own entry, and kept the same way.
    let gzip = shell.get(path, Some("gzip")).await;
    assert_eq!(gunzip(&gzip.body), changing(1).as_bytes());
    assert_eq!(shell.compressed.held().entries, settled.entries + 1);
    let gzip_again = shell.get(path, Some("gzip")).await;
    assert_eq!(gzip_again.body, gzip.body);
    assert_eq!(shell.compressed.held().entries, settled.entries + 1);
}

#[tokio::test]
async fn the_same_bytes_from_anywhere_are_kept_once() {
    let shell = shell().await;
    // The frontend and the module serve the same wasm here.
    let held = shell.settled(1).await;
    let module = shell.get(WASM, BROWSER).await;
    assert_eq!(unbrotli(&module.body), wasm());
    assert_eq!(module.body.len(), held.bytes);
    assert_eq!(shell.compressed.held(), held);
}

#[tokio::test]
async fn a_changed_file_is_compressed_afresh() {
    let shell = shell().await;
    let path = "/m/checklist/ui/items/changing.js";

    let before = shell.get(path, BROWSER).await;
    assert_eq!(before.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&before.body), changing(1).as_bytes());

    // Deployed again, under the same name.
    *shell.version.lock().unwrap() = 2;
    let after = shell.get(path, BROWSER).await;
    assert_eq!(after.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&after.body), changing(2).as_bytes());
}

#[tokio::test]
async fn the_frontend_is_compressed_at_its_best_before_anybody_asks() {
    let shell = shell().await;
    let held = shell.settled(1).await;
    assert_eq!(
        held.entries, 1,
        "the wasm, and not an index.html under 1 KB"
    );

    let answer = shell.get("/hlin-ui_bg.wasm", BROWSER).await;
    assert_eq!(answer.header(header::CONTENT_ENCODING), Some("br"));
    assert_eq!(unbrotli(&answer.body), wasm());
    assert_eq!(
        answer.body.len(),
        held.bytes,
        "what was kept, as it was kept"
    );
    assert_eq!(shell.compressed.held(), held);
}

#[tokio::test]
async fn a_file_bigger_than_the_whole_budget_is_sent_as_it_is() {
    let shell = shell_keeping(CompressionConfig { cache_bytes: 8192 }).await;
    let big = shell.get(WASM, BROWSER).await;
    assert_eq!(big.status, StatusCode::OK);
    assert!(big.header(header::CONTENT_ENCODING).is_none());
    assert_eq!(big.body, wasm());
    assert!(big.varies_by_encoding());

    // One that fits is still compressed.
    let small = shell
        .get("/m/checklist/ui/items/changing.js", BROWSER)
        .await;
    assert_eq!(small.header(header::CONTENT_ENCODING), Some("br"));
    assert!(shell.compressed.held().bytes <= 8192);
}

#[tokio::test]
async fn a_budget_of_nothing_turns_compression_off() {
    let shell = shell_keeping(CompressionConfig { cache_bytes: 0 }).await;
    for path in [WASM, "/hlin-ui_bg.wasm"] {
        let answer = shell.get(path, BROWSER).await;
        assert_eq!(answer.status, StatusCode::OK);
        assert!(answer.header(header::CONTENT_ENCODING).is_none(), "{path}");
        assert_eq!(answer.body, wasm());
    }
    assert_eq!(shell.compressed.held().entries, 0);
}
