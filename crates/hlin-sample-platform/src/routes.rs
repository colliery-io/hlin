//! The HTTP surface: a manifest, a data endpoint per panel, and health.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hlin_manifest::envelope::Envelope;
use serde::Deserialize;

use crate::changes::Changes;
use crate::data::{self, Window};

/// How this platform decides who is calling.
///
/// Both are strategies from [[HLIN-S-0005]] seen from the platform's side. The
/// point of supporting both is that a demo can stand them side by side: one
/// platform that Hlin reaches with no change at all, and one that verifies a
/// signed token.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum IdentityMode {
    /// Trust a session cookie, taking its value as the principal. What a
    /// platform already behind the shared session does for free.
    Session {
        /// The cookie carrying the principal.
        cookie: String,
    },
    /// Verify the shell's signed token against its published keys. What a
    /// platform does once it has adopted the contract.
    Token {
        /// Verifies tokens from the shell that calls this platform.
        verifier: Arc<hlin_identity::Verifier>,
    },
    /// Accept anything. Development only, and refused unless explicitly asked
    /// for, so it cannot be reached by forgetting a flag.
    Open,
}

/// How this platform was started.
#[derive(Clone)]
pub struct Config {
    /// The platform's id, which is also its `platform.id`.
    pub name: String,
    /// Whether to serve the manifest that drops a panel without a major bump.
    pub breaking: bool,
    /// How callers are identified.
    pub identity: IdentityMode,
    /// A group required to read `worker-health`, so `forbidden` is demoable.
    pub restricted_panel_group: Option<String>,
    /// The state this platform actually mutates, and its notifications.
    pub changes: Arc<Changes>,
}

/// The router for a platform with this configuration.
pub fn router(config: Config) -> Router {
    // The options endpoint carries the platform name, so two instances on one
    // shell cannot collide. It is registered as a literal because the platform
    // knows its own name here, and a path parameter cannot be part of a
    // segment.
    let options_path = format!("/api/hlin/{}-clusters", config.name);
    let state = Arc::new(config);

    Router::new()
        .route(&options_path, get(cluster_options))
        .route("/.well-known/hlin.json", get(manifest))
        .route("/api/health", get(health))
        .route("/api/hlin/records-per-second", get(records_per_second))
        .route("/api/hlin/live-rate", get(live_rate))
        .route("/api/hlin/live-throughput", get(live_throughput))
        .route("/api/hlin/saturation", get(saturation))
        .route("/api/hlin/pipeline", get(pipeline))
        .route("/api/hlin/throughput", get(throughput))
        .route(
            "/api/hlin/throughput-by-cluster",
            get(throughput_by_cluster),
        )
        .route("/api/hlin/queue-depth", get(queue_depth))
        .route("/api/hlin/worker-health", get(worker_health))
        .route("/api/hlin/batches", get(batches))
        .route("/api/events", get(events))
        .with_state(state)
}

/// The manifest, served to anyone.
///
/// Deliberately not behind identity: the shell fetches it as itself, and it is
/// the same document for every viewer ([[HLIN-S-0001]] REQ-1.1).
async fn manifest(State(config): State<Arc<Config>>) -> Json<serde_json::Value> {
    let document = crate::manifest::build(&config.name, config.breaking);
    Json(serde_json::to_value(document).expect("the manifest serialises"))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Who is asking, or a refusal.
///
/// The 401 and 403 distinction is the one platforms most often get wrong, and
/// the shell depends on it: 401 becomes `malformed` and tells an operator
/// something is misconfigured, while 403 becomes `forbidden` and tells the
/// viewer they need access ([[HLIN-S-0004]]).
struct Caller {
    principal: String,
    groups: Vec<String>,
}

fn identify(config: &Config, headers: &HeaderMap) -> Result<Caller, Refusal> {
    let caller = identify_inner(config, headers);
    match &caller {
        Ok(who) => tracing::debug!(principal = %who.principal, "serving"),
        Err(_) => tracing::debug!("refused a caller"),
    }
    caller
}

fn identify_inner(config: &Config, headers: &HeaderMap) -> Result<Caller, Refusal> {
    match &config.identity {
        IdentityMode::Open => Ok(Caller {
            principal: "anonymous".to_string(),
            groups: vec![],
        }),

        IdentityMode::Session { cookie } => {
            let jar = headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");

            match cookie_value(jar, cookie) {
                Some(principal) => Ok(Caller {
                    // The demo's session cookie carries `principal:group,group`
                    // so one platform can demonstrate a 403 without a real
                    // identity provider.
                    groups: principal
                        .split_once(':')
                        .map(|(_, groups)| {
                            groups.split(',').map(str::to_string).collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    principal: principal
                        .split_once(':')
                        .map(|(who, _)| who.to_string())
                        .unwrap_or(principal),
                }),
                None => Err(refuse(StatusCode::UNAUTHORIZED, "no session")),
            }
        }

        IdentityMode::Token { verifier } => {
            let presented = headers
                .get(hlin_identity::IDENTITY_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");

            // One call checks the signature, the issuer, the audience and the
            // expiry. There is no way to do part of that, which is the point:
            // a platform that checked only the signature would accept a token
            // minted for somebody else.
            match verifier.verify(presented, &config.name) {
                Ok(claims) => Ok(Caller {
                    principal: claims.sub.clone(),
                    groups: claims.groups.clone(),
                }),
                Err(reason) => {
                    // A refused credential is a 401. Whether this principal may
                    // see a panel is a separate question, answered below with a
                    // 403, and conflating the two makes a broken deployment
                    // look like a permissions problem.
                    tracing::warn!(%reason, "refused an identity");
                    Err(refuse(
                        StatusCode::UNAUTHORIZED,
                        "identity was not accepted",
                    ))
                }
            }
        }
    }
}

fn cookie_value(jar: &str, name: &str) -> Option<String> {
    jar.split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

/// Why a caller was turned away.
///
/// Kept small and typed rather than a built response, so the decision and its
/// rendering stay separable and a handler cannot return a refusal with a status
/// that contradicts its reason.
struct Refusal {
    status: StatusCode,
    reason: &'static str,
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.reason })),
        )
            .into_response()
    }
}

fn refuse(status: StatusCode, reason: &'static str) -> Refusal {
    Refusal { status, reason }
}

fn envelope(document: Envelope) -> Response {
    Json(serde_json::to_value(document).expect("an envelope serialises")).into_response()
}

/// The parameters the shell encodes onto a data request ([[HLIN-S-0002]]).
#[derive(Debug, Deserialize, Default)]
struct Params {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    step: Option<i64>,
    cluster: Option<String>,
}

impl Params {
    fn window(&self) -> Window {
        Window::new(self.from, self.to, self.step)
    }
}

async fn records_per_second(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::records_per_second(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

async fn saturation(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::saturation(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

async fn pipeline(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::pipeline(&config.changes.stages())),
        Err(refusal) => refusal.into_response(),
    }
}

async fn live_rate(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::live_rate(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

/// No `Query<Params>`: this endpoint answers a window that ends now, and taking
/// a time range it intends to ignore would be a promise it does not keep.
async fn live_throughput(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::live_throughput(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

async fn throughput(
    State(config): State<Arc<Config>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::throughput(&config.name, params.window())),
        Err(refusal) => refusal.into_response(),
    }
}

/// The cluster is read; the time range is not, because this endpoint answers a
/// window that ends now. Taking a range it intends to ignore would be a promise
/// it does not keep, which is the rule `live-throughput` already follows.
async fn throughput_by_cluster(
    State(config): State<Arc<Config>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::throughput_by_cluster(
            &config.name,
            params.cluster.as_deref(),
        )),
        Err(refusal) => refusal.into_response(),
    }
}

async fn queue_depth(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::queue_depth(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

/// The one panel with a rule of its own, so the `forbidden` state has something
/// to be demonstrated by.
async fn worker_health(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    let caller = match identify(&config, &headers) {
        Ok(caller) => caller,
        Err(refusal) => return refusal.into_response(),
    };

    if let Some(required) = &config.restricted_panel_group
        && !caller.groups.iter().any(|group| group == required)
    {
        return refuse(
            StatusCode::FORBIDDEN,
            "this panel is restricted to on-call staff",
        )
        .into_response();
    }

    envelope(data::worker_health(&config.name))
}

async fn cluster_options(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::cluster_options(&config.name)),
        Err(refusal) => refusal.into_response(),
    }
}

async fn batches(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    match identify(&config, &headers) {
        Ok(_) => envelope(data::batches(config.changes.batches())),
        Err(refusal) => refusal.into_response(),
    }
}

/// This platform saying which of its panels have just changed.
///
/// The whole of a platform's side of [[HLIN-S-0006]], and deliberately small: a
/// held-open response that names a panel key when one of them moves. It carries
/// no data, because a frame is a property of a panel *and* who is asking *and*
/// what they selected, and this platform knows only the first of those.
///
/// Behind identity like every data endpoint. A stream that says which panels are
/// changing is a stream that says something about this platform's traffic, and
/// there is no reason for it to be the one thing readable by anybody.
async fn events(State(config): State<Arc<Config>>, headers: HeaderMap) -> Response {
    if let Err(refusal) = identify(&config, &headers) {
        return refusal.into_response();
    }

    // Subscribed before the response is returned, so a change between now and
    // the first poll of the stream is queued rather than missed.
    let mut changed = config.changes.subscribe();

    let stream = async_stream::stream! {
        loop {
            match changed.recv().await {
                Ok(panel) => {
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default()
                            .event("changed")
                            .data(serde_json::json!({ "panel": panel }).to_string()),
                    );
                }
                // A subscriber that fell behind has missed some notices. It
                // needs no catch-up: the shell polls underneath, so the worst
                // case is one relaxed interval of staleness, and telling it
                // *something* changed is both true and enough.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                    // Everything this platform reports on, because which
                    // notices were missed is exactly what is not known. Telling
                    // the shell about all of them is both true and cheap: it
                    // refetches what it is watching and nothing else.
                    tracing::debug!(missed, "a subscriber fell behind");
                    for panel in crate::changes::PUSHED_PANELS {
                        yield Ok(axum::response::sse::Event::default()
                            .event("changed")
                            .data(serde_json::json!({ "panel": panel }).to_string()));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    // The heartbeat is required, not a nicety (HLIN-S-0006). A connection can
    // stop delivering without closing — a NAT table forgets it, a proxy drops
    // what looks idle — and nothing announces that. Without this the shell would
    // hold a socket it believes is live and poll less on a promise nobody is
    // keeping, which is the one way this feature could leave a viewer worse off
    // than plain polling.
    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response()
}
