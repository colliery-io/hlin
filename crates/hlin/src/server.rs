//! The shell's HTTP surface.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::config::Config;
use crate::registry::Registry;

/// Everything a handler needs.
#[derive(Clone)]
pub struct AppState {
    /// What the shell was told.
    pub config: Arc<Config>,
    /// What it has learned.
    pub registry: Arc<Registry>,
    /// The key it signs with.
    pub issuer: Arc<hlin_identity::Issuer>,
    /// The surfaces currently being served.
    pub surfaces: Arc<crate::surfaces::Surfaces>,
    /// What it remembers: contracts, and the layouts people composed.
    pub store: Arc<dyn crate::store::Store>,
    /// One client for the shell.s own upstream calls, so a connection pool is
    /// shared rather than a new one built per request.
    pub client: reqwest::Client,

    /// A second client, for connections that are meant to stay open.
    ///
    /// The one above carries the upstream timeout, which is right for a data
    /// fetch and fatal for an event stream: a request timeout applies to the
    /// whole exchange including the body, so a held-open response is killed the
    /// moment it outlives it. Measured before this existed — every subscription
    /// died after ten seconds and reconnected forever, and the saving the
    /// feature exists for never appeared.
    ///
    /// It keeps a connect timeout, because failing to *reach* a platform should
    /// still give up.
    pub stream_client: reqwest::Client,

    /// Every platform event stream the shell holds, shared across surfaces.
    ///
    /// One connection per platform for the whole shell (HLIN-S-0006 REQ-2.1),
    /// which is why it lives on the shell rather than on a surface.
    pub streams: std::sync::Arc<crate::stream::streams::Streams>,
}

/// The shell's routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/platforms", get(platforms))
        .route(
            &format!("/{}", hlin_identity::JWKS_PATH),
            get(published_keys),
        )
        .route("/api/stream/{surface_id}", get(stream))
        .route("/api/stream/{surface_id}/params", post(set_params))
        .route("/api/config", get(client_config))
        // Composition. `home` is a literal and is registered before the
        // parameterised route it would otherwise be swallowed by.
        .route("/api/panels", get(crate::layouts::catalog))
        .route(
            "/api/options/{platform_id}/{panel_key}/{control_id}",
            get(crate::options::choices),
        )
        .route(
            "/api/layouts",
            get(crate::layouts::list).post(crate::layouts::create),
        )
        .route("/api/layouts/home", get(crate::layouts::home))
        .route(
            "/api/layouts/{id}",
            get(crate::layouts::read)
                .put(crate::layouts::replace)
                .delete(crate::layouts::remove),
        )
        .route("/api/layouts/{id}/fork", post(crate::layouts::fork))
        // Empty unless the shell authenticates people itself. A shell told who
        // everyone is by a proxy has nothing to offer on `/auth/login`, and a
        // route that exists and cannot work is worse than one that does not.
        .merge(crate::auth::routes(&state.config.auth))
        // A visitor cookie, where nobody signs in. Outside `anonymous` the
        // layer returns immediately, so it costs a match on every request and
        // nothing else; it is added unconditionally because a router that
        // changes shape with configuration is a router two deployments cannot
        // be reasoned about together.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::visitor::assign,
        ))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// What the browser needs to know about this shell.
async fn client_config(
    State(state): State<AppState>,
    crate::identity::Caller(principal): crate::identity::Caller,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "protocol_version": crate::stream::PROTOCOL_VERSION,
        "stream_loss_grace_seconds": state.config.timings.stream_loss_grace().num_seconds(),
        "refresh_ms": state.config.timings.refresh_ms,
        "principal": { "sub": principal.sub, "name": principal.name },
        // So the front end can stop offering what cannot succeed. A browser
        // that learned this by trying would show an Edit button, let somebody
        // arrange a surface, and refuse at Save — which is the worst possible
        // moment to say it.
        "read_only": state.config.read_only(),
    }))
}

/// The keys that verify this shell's tokens.
///
/// Unauthenticated: public keys are public, and requiring a credential to
/// fetch them would create the bootstrapping problem the design avoids
/// ([[HLIN-S-0004]]).
async fn published_keys(State(state): State<AppState>) -> Json<hlin_identity::Jwks> {
    Json(state.issuer.jwks())
}

/// What the shell currently knows about every configured platform.
///
/// Also the panel picker's source, so its shape is designed once here and read
/// by the frontend later.
#[derive(Debug, Serialize)]
pub struct PlatformReport {
    /// As configured.
    pub id: String,
    /// Its display name, once a manifest has been read.
    pub name: Option<String>,
    /// Whether it answered the last time it was asked.
    pub reachable: bool,
    /// Why not, when it did not.
    pub trouble: Option<String>,
    /// What it declares as its contract version.
    pub contract_version: Option<String>,
    /// The fingerprint the shell computed.
    pub contract_hash: Option<String>,
    /// How the shell identifies itself to this platform.
    pub credential: &'static str,
    /// How many consecutive polls have agreed on the current contract.
    pub consecutive_observations: i32,
    /// The last breaking change this platform shipped without declaring it,
    /// if it ever has. Absent for a platform that has behaved.
    pub last_violation: Option<crate::store::Violation>,
    /// The panels a viewer may use.
    ///
    /// The same shape the picker reads from `/api/panels`, so an operator's
    /// view of a panel and a composer's view of it cannot drift apart.
    pub panels: Vec<hlin_stream::layout::CatalogPanel>,
    /// The panels this platform declared that the shell will not offer.
    pub rejected: Vec<RejectedPanel>,

    /// What its event stream is doing, if it offers one.
    ///
    /// Here rather than in the log because a stream that has quietly died is
    /// invisible from every other angle: the panels still draw, and they draw
    /// less currently than anybody watching believes. An operator asking why a
    /// dashboard feels behind has nowhere else to look.
    pub events: crate::stream::streams::StreamHealth,
}

/// A panel the shell will not offer, and why.
#[derive(Debug, Serialize)]
pub struct RejectedPanel {
    /// The key it declared.
    pub key: String,
    /// Why it was rejected, in words for an operator.
    pub reason: String,
}

async fn platforms(State(state): State<AppState>) -> Json<Vec<PlatformReport>> {
    let views = state.registry.views().await;
    let streams = state.streams.health().await;

    let reports = views
        .values()
        .map(|view| {
            let panels = view
                .accepted_panels()
                .into_iter()
                .map(|panel| crate::layouts::catalog_panel(&view.config.id, panel))
                .collect();

            let rejected = view
                .validation
                .as_ref()
                .map(|validation| {
                    validation
                        .rejected()
                        .into_iter()
                        .map(|(key, defect)| RejectedPanel {
                            key: key.to_string(),
                            reason: defect.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            PlatformReport {
                id: view.config.id.clone(),
                name: view.manifest.as_ref().map(|m| m.platform.name.clone()),
                reachable: view.reachable,
                trouble: view.trouble.clone(),
                contract_version: view
                    .manifest
                    .as_ref()
                    .map(|m| m.contract_version.to_string()),
                contract_hash: view.contract_hash.clone(),
                credential: view.credentialer.name(),
                consecutive_observations: view.consecutive_observations,
                last_violation: view.last_violation.clone(),
                // A platform that declares a stream nobody is currently
                // watching has no entry, and reports as not connected with
                // `declared` true — which is the truth, and different from a
                // platform that offers nothing at all.
                events: streams.get(&view.config.id).cloned().unwrap_or_else(|| {
                    let declares = view
                        .manifest
                        .as_ref()
                        .is_some_and(|manifest| manifest.events.is_some());
                    crate::stream::streams::StreamHealth {
                        declared: declares,
                        ..crate::stream::streams::StreamHealth::none_offered()
                    }
                }),
                panels,
                rejected,
            }
        })
        .collect();

    Json(reports)
}

// -- The stream ------------------------------------------------------------

use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::post;
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::broadcast;

/// Serve a surface's frames as server-sent events.
///
/// One stream per surface, carrying every panel on it: a browser holding
/// twelve connections is what this design exists to avoid (HLIN-S-0003
/// REQ-1.1).
async fn stream(
    State(state): State<AppState>,
    crate::identity::Caller(principal): crate::identity::Caller,
    Path(surface_id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let live = state
        .surfaces
        .for_surface(&surface_id, &principal, &state, &headers)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let mut receiver = live.subscribe();
    // Everything the browser needs to be correct, before any new frame
    // arrives; a reconnecting browser needs no replay.
    let current = live.current().await;

    let stream = async_stream::stream! {
        for frame in current {
            yield Ok(sse(&frame));
        }

        loop {
            match receiver.recv().await {
                Ok(frame) => yield Ok(sse(&frame)),

                // A subscriber that fell more than the buffer behind. Skipping
                // is correct rather than merely tolerable: a browser applies
                // the newest generation and drops the rest, so the frames it
                // missed are ones it would have discarded.
                //
                // `while let Ok(..)` treated this like a closed channel and
                // ended the stream. The surface then healed by accident —
                // `EventSource` reconnects and `current_frames` resends
                // everything — which hid the cost: under sustained slowness
                // that is a reconnect loop, each one a `for_surface` call and a
                // full-state send, for a client that only needed to skip ahead.
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::debug!(
                        surface = surface_id,
                        missed,
                        "a browser fell behind; skipping to what is current"
                    );

                    // Sent because the skipped frames may have carried a state
                    // change this browser has now missed entirely. Cheaper than
                    // the reconnect this used to cause, and leaves the browser
                    // whole rather than merely caught up.
                    for frame in live.current().await {
                        yield Ok(sse(&frame));
                    }
                }

                // The surface has stopped. Ending the stream is right: the
                // browser reconnects and gets a fresh one.
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn sse(frame: &crate::stream::Frame) -> Event {
    Event::default().event(frame.event()).data(frame.to_json())
}

/// A control moved.
async fn set_params(
    State(state): State<AppState>,
    crate::identity::Caller(principal): crate::identity::Caller,
    Path(surface_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<crate::stream::ParamsRequest>,
) -> impl IntoResponse {
    match state
        .surfaces
        .for_surface(&surface_id, &principal, &state, &headers)
        .await
    {
        Ok(live) => {
            live.set_params(request).await;
            StatusCode::ACCEPTED
        }
        Err(_) => StatusCode::NOT_FOUND,
    }
}
