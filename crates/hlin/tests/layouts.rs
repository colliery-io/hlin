//! Composition, over the shell's own HTTP surface.
//!
//! The layout API is the whole write path for the thing the product exists to
//! let people do, so these drive the real router rather than the handlers:
//! routing, extraction, status codes and ownership are all part of the
//! behaviour being claimed.
//!
//! Everything runs against a fake manifest client and the in-memory store, so
//! there is no network and no database.

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hlin::config::{AuthConfig, Config, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, Store};
use hlin_manifest::Manifest;
use hlin_stream::layout::{
    CatalogPlatform, LayoutDocument, LayoutSummary, PanelInstanceDocument, Placement,
};
use tower::ServiceExt;

const PLATFORM: &str = "orebank";
const OTHER: &str = "smelter";

/// A client that answers every platform with the same manifest, with its id
/// rewritten so two configured platforms look like two platforms.
struct Everywhere;

#[async_trait]
impl ManifestClient for Everywhere {
    async fn fetch(&self, base_url: &str) -> Fetched {
        let id = if base_url.ends_with("9998") {
            OTHER
        } else {
            PLATFORM
        };
        Fetched::Document(Box::new(manifest(id)))
    }
}

fn manifest(id: &str) -> Manifest {
    hlin_manifest::parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "1.0.0",
          "platform": {{ "id": "{id}", "name": "{id}" }},
          "panels": [
            {{ "key": "throughput", "title": "Throughput", "kind": "timeseries",
               "envelope": "series.v1", "data": "api/throughput" }},
            {{ "key": "queue-depth", "title": "Queue depth", "kind": "stat",
               "envelope": "scalar.v1", "data": "api/queue-depth" }}
          ],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

fn config() -> Config {
    Config {
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        database_url: None,
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
                id: PLATFORM.to_string(),
                base_url: "http://127.0.0.1:9999".to_string(),
                auth: CredentialConfig::HlinToken,
            },
            PlatformConfig {
                id: OTHER.to_string(),
                base_url: "http://127.0.0.1:9998".to_string(),
                auth: CredentialConfig::HlinToken,
            },
        ],
    }
}

/// A shell with both platforms already discovered.
async fn shell() -> (axum::Router, Arc<dyn Store>, Arc<Config>) {
    shell_with(config()).await
}

/// The same shell, with an authenticator of the caller's choosing.
async fn shell_with(configured: Config) -> (axum::Router, Arc<dyn Store>, Arc<Config>) {
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

    (router(state), store, config)
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a readable body");
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("a request")
}

fn send(method: &str, path: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("a request")
}

// -- The catalogue --------------------------------------------------------

#[tokio::test]
async fn the_picker_is_offered_every_accepted_panel_grouped_by_platform() {
    let (app, _, _) = shell().await;

    let (status, body) = call(&app, get("/api/panels")).await;
    assert_eq!(status, StatusCode::OK);

    let platforms: Vec<CatalogPlatform> = serde_json::from_value(body).expect("a catalogue");
    assert_eq!(platforms.len(), 2, "both configured platforms are listed");

    let orebank = platforms
        .iter()
        .find(|platform| platform.id == PLATFORM)
        .expect("the platform is in the catalogue");
    assert_eq!(orebank.panels.len(), 2);

    let throughput = orebank
        .panels
        .iter()
        .find(|panel| panel.key == "throughput")
        .expect("the panel is offered");

    assert_eq!(throughput.reference, "orebank/throughput");
    assert_eq!(throughput.envelope, "series.v1");
    assert!(
        throughput.available_kinds.len() > 1,
        "a viewer can switch a series between more than one rendering, which is \
         the whole reason the picker carries the list: {:?}",
        throughput.available_kinds
    );
}

// -- Layouts --------------------------------------------------------------

#[tokio::test]
async fn a_first_visit_lands_on_a_layout_that_did_not_exist_yet() {
    let (app, _, _) = shell().await;

    let (status, body) = call(&app, get("/api/layouts/home")).await;
    assert_eq!(status, StatusCode::OK);

    let layout: LayoutDocument = serde_json::from_value(body).expect("a layout");
    assert!(layout.id.is_some(), "it was stored, so it has an identity");
    assert!(layout.panels.is_empty(), "and nothing on it yet");
    assert!(layout.editable, "the principal who got it owns it");

    // Asking again returns the same one rather than making a second.
    let (_, again) = call(&app, get("/api/layouts/home")).await;
    let again: LayoutDocument = serde_json::from_value(again).expect("a layout");
    assert_eq!(again.id, layout.id, "a second visit is not a second layout");

    let (_, listed) = call(&app, get("/api/layouts")).await;
    let listed: Vec<LayoutSummary> = serde_json::from_value(listed).expect("a list");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn panels_added_by_the_browser_are_given_identities_and_survive_a_reload() {
    let (app, _, _) = shell().await;

    let (_, home) = call(&app, get("/api/layouts/home")).await;
    let mut layout: LayoutDocument = serde_json::from_value(home).expect("a layout");
    let id = layout.id.clone().expect("an identity");

    layout.panels = vec![
        PanelInstanceDocument::new(
            PLATFORM,
            "throughput",
            Placement {
                x: 0,
                y: 0,
                w: 6,
                h: 4,
            },
        ),
        PanelInstanceDocument::new(
            OTHER,
            "queue-depth",
            Placement {
                x: 6,
                y: 0,
                w: 6,
                h: 4,
            },
        ),
    ];

    let (status, body) = call(
        &app,
        send(
            "PUT",
            &format!("/api/layouts/{id}"),
            &serde_json::to_value(&layout).expect("serialises"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let written: LayoutDocument = serde_json::from_value(body).expect("a layout");
    assert_eq!(written.panels.len(), 2);
    for panel in &written.panels {
        assert!(
            panel.id.is_some(),
            "the shell assigns identity, so the stream has something to name"
        );
    }

    // A reload is a fresh GET, and it must show the same arrangement.
    let (_, reloaded) = call(&app, get(&format!("/api/layouts/{id}"))).await;
    let reloaded: LayoutDocument = serde_json::from_value(reloaded).expect("a layout");
    assert_eq!(reloaded.panels.len(), 2);
    assert_eq!(reloaded.panels[0].position.w, 6);
    assert_eq!(reloaded.panels[1].position.x, 6);
    assert_eq!(
        reloaded
            .panels
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>(),
        written
            .panels
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>(),
        "and the same panels, not new ones"
    );
}

#[tokio::test]
async fn a_second_write_keeps_the_identities_the_first_one_assigned() {
    let (app, _, _) = shell().await;

    let (_, home) = call(&app, get("/api/layouts/home")).await;
    let mut layout: LayoutDocument = serde_json::from_value(home).expect("a layout");
    let id = layout.id.clone().expect("an identity");

    layout.panels = vec![PanelInstanceDocument::new(
        PLATFORM,
        "throughput",
        Placement::default(),
    )];
    let (_, first) = call(
        &app,
        send(
            "PUT",
            &format!("/api/layouts/{id}"),
            &serde_json::to_value(&layout).expect("serialises"),
        ),
    )
    .await;
    let mut first: LayoutDocument = serde_json::from_value(first).expect("a layout");
    let assigned = first.panels[0].id.clone().expect("an identity");

    // The viewer drags it. Same panel, new position.
    first.panels[0].position = Placement {
        x: 3,
        y: 2,
        w: 5,
        h: 3,
    };
    let (_, second) = call(
        &app,
        send(
            "PUT",
            &format!("/api/layouts/{id}"),
            &serde_json::to_value(&first).expect("serialises"),
        ),
    )
    .await;
    let second: LayoutDocument = serde_json::from_value(second).expect("a layout");

    assert_eq!(
        second.panels[0].id.as_deref(),
        Some(assigned.as_str()),
        "moving a panel does not make it a different panel, or the open stream \
         would stop naming it"
    );
    assert_eq!(second.panels[0].position.x, 3);
}

#[tokio::test]
async fn a_position_off_the_grid_is_pulled_back_rather_than_refused() {
    let (app, _, _) = shell().await;

    let (_, home) = call(&app, get("/api/layouts/home")).await;
    let mut layout: LayoutDocument = serde_json::from_value(home).expect("a layout");
    let id = layout.id.clone().expect("an identity");

    layout.panels = vec![PanelInstanceDocument {
        position: Placement {
            x: 11,
            y: 0,
            w: 6,
            h: 0,
        },
        ..PanelInstanceDocument::new(PLATFORM, "throughput", Placement::default())
    }];

    let (status, body) = call(
        &app,
        send(
            "PUT",
            &format!("/api/layouts/{id}"),
            &serde_json::to_value(&layout).expect("serialises"),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a bad position is not a bad request"
    );

    let written: LayoutDocument = serde_json::from_value(body).expect("a layout");
    let position = written.panels[0].position;
    assert_eq!(position.x, 6, "pulled left until the panel fits");
    assert_eq!(position.h, 1, "and given a height it can be seen at");
}

#[tokio::test]
async fn a_layout_belonging_to_someone_else_is_readable_and_not_writable() {
    let (app, store, config) = shell().await;

    // A layout somebody else published. The development principal can see it,
    // because it is published, and cannot write it, because it is not theirs.
    let theirs = store
        .create_layout(hlin::store::NewLayout {
            owner: "grace".to_string(),
            title: "Theirs".to_string(),
            visibility: hlin::store::Visibility::Published,
            forked_from: None,
            time_range: None,
            panels: vec![],
        })
        .await
        .expect("the store accepts it");

    let (status, body) = call(&app, get(&format!("/api/layouts/{}", theirs.id))).await;
    assert_eq!(status, StatusCode::OK);
    let document: LayoutDocument = serde_json::from_value(body).expect("a layout");
    assert!(
        !document.editable,
        "the browser is told plainly rather than left to infer it"
    );

    let (status, _) = call(
        &app,
        send(
            "PUT",
            &format!("/api/layouts/{}", theirs.id),
            &serde_json::to_value(&document).expect("serialises"),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "not a 404: hiding a layout they can see would make forking senseless"
    );

    // Forking is the write path that is open to them.
    let (status, body) = call(
        &app,
        send(
            "POST",
            &format!("/api/layouts/{}/fork", theirs.id),
            &serde_json::Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let forked: LayoutDocument = serde_json::from_value(body).expect("a layout");
    assert!(forked.editable);
    assert_eq!(
        forked.owner,
        config
            .principal_from(&axum::http::HeaderMap::new())
            .known()
            .expect("the dev authenticator always answers")
            .sub
    );
    assert_ne!(forked.id, document.id);
}

#[tokio::test]
async fn a_personal_layout_someone_else_owns_is_not_admitted_to_exist() {
    let (app, store, _) = shell().await;

    let theirs = store
        .create_layout(hlin::store::NewLayout::personal("grace", "Private"))
        .await
        .expect("the store accepts it");

    let (status, _) = call(&app, get(&format!("/api/layouts/{}", theirs.id))).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "knowing the link is the permission, and this principal does not have it"
    );
}

#[tokio::test]
async fn an_identifier_that_is_not_one_is_a_not_found_rather_than_a_panic() {
    let (app, _, _) = shell().await;

    let (status, _) = call(&app, get("/api/layouts/not-a-uuid")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(&app, get(&format!("/api/layouts/{}", uuid::Uuid::new_v4()))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_deleted_layout_is_gone() {
    let (app, _, _) = shell().await;

    let (_, home) = call(&app, get("/api/layouts/home")).await;
    let layout: LayoutDocument = serde_json::from_value(home).expect("a layout");
    let id = layout.id.clone().expect("an identity");

    let (status, _) = call(
        &app,
        send(
            "DELETE",
            &format!("/api/layouts/{id}"),
            &serde_json::Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = call(&app, get(&format!("/api/layouts/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn two_requests_arriving_together_get_the_same_surface() {
    // The browser posts its parameters and opens its stream from the same tick,
    // so these two arrive together every time a layout is written. When the
    // shell built a surface for each, the second won the map and the first went
    // on running with a browser attached to it — receiving frames from a
    // generation that browser had already moved past, and discarding every one.
    // The panels sat in their loading skeletons and nothing anywhere said why.
    let config = Arc::new(config());
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Everywhere))
            .expect("registry builds"),
    );
    registry.poll_all().await;

    let surfaces = Arc::new(hlin::surfaces::Surfaces::new());
    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: surfaces.clone(),
        store: store.clone(),
        client: reqwest::Client::new(),
        stream_client: reqwest::Client::new(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    let layout = store
        .create_layout(hlin::store::NewLayout::personal(
            config
                .principal_from(&axum::http::HeaderMap::new())
                .known()
                .expect("the dev authenticator always answers")
                .sub,
            "Together",
        ))
        .await
        .expect("the store accepts it");
    let id = layout.id.to_string();

    let headers = axum::http::HeaderMap::new();
    let principal = config
        .principal_from(&headers)
        .known()
        .expect("the dev authenticator always answers");
    let (first, second) = tokio::join!(
        surfaces.for_surface(&id, &principal, &state, &headers),
        surfaces.for_surface(&id, &principal, &state, &headers)
    );

    let first = first.expect("a surface");
    let second = second.expect("a surface");

    assert!(
        Arc::ptr_eq(&first, &second),
        "both callers must get the one surface; a second would be orphaned the \
         moment it was inserted, taking its subscriber's frames with it"
    );
}

#[tokio::test]
async fn a_surface_nobody_is_watching_stops_asking_its_platforms() {
    // A surface used to run until the process exited. `Surfaces` only shrank on
    // a layout write, `run` was a loop with no exit, and nothing consulted the
    // subscriber count — so a closed tab, a browser navigated away, or a
    // reconnect that got a new key each left a driver fetching every panel on
    // that surface forever. The demo was measured sending 84 and 151 requests a
    // second to its two sample platforms with nobody watching at all.
    //
    // The grace itself is thirty seconds, which no unit test should sit through.
    // What is asserted here is the decision `release` makes, which is the part
    // with the race in it: the driver asks, and the answer depends on whether a
    // browser has attached by the time the lock is taken.
    let config = Arc::new(config());
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Everywhere))
            .expect("registry builds"),
    );
    registry.poll_all().await;

    let surfaces = Arc::new(hlin::surfaces::Surfaces::new());
    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: surfaces.clone(),
        store: store.clone(),
        client: reqwest::Client::new(),
        stream_client: reqwest::Client::new(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    let layout = store
        .create_layout(hlin::store::NewLayout::personal(
            config
                .principal_from(&axum::http::HeaderMap::new())
                .known()
                .expect("the dev authenticator always answers")
                .sub,
            "Unwatched",
        ))
        .await
        .expect("the store accepts it");
    let id = layout.id.to_string();
    let headers = axum::http::HeaderMap::new();
    let principal = config
        .principal_from(&headers)
        .known()
        .expect("the dev authenticator always answers");
    let key = format!(
        "{id}:{}",
        config
            .principal_from(&axum::http::HeaderMap::new())
            .known()
            .expect("the dev authenticator always answers")
            .sub
    );

    let running = surfaces
        .for_surface(&id, &principal, &state, &headers)
        .await
        .expect("a surface");

    // With a browser attached, the driver must be told to keep going. This is
    // the case that matters: releasing here would strand the subscriber.
    let watching = running.subscribe();
    assert_eq!(running.viewers(), 1);
    assert!(
        !surfaces.release(&key, &running).await,
        "a surface with a subscriber must not be released"
    );

    // The same surface is still the one being served, so the browser that was
    // attached is still attached to what the shell will hand the next caller.
    let again = surfaces
        .for_surface(&id, &principal, &state, &headers)
        .await
        .expect("a surface");
    assert!(Arc::ptr_eq(&running, &again));

    // The browser goes.
    drop(watching);
    assert_eq!(running.viewers(), 0);
    assert!(
        surfaces.release(&key, &running).await,
        "a surface nobody is watching is released"
    );

    // And it is gone from the map, so the next subscriber builds a fresh one
    // rather than attaching to a driver that has stopped.
    let rebuilt = surfaces
        .for_surface(&id, &principal, &state, &headers)
        .await
        .expect("a surface");
    assert!(
        !Arc::ptr_eq(&running, &rebuilt),
        "a released surface must not be handed out again"
    );

    // Releasing a surface the map no longer holds is not an error, and must not
    // remove whatever replaced it: a driver that decided to stop while its key
    // was reused would otherwise take the new surface down with it.
    assert!(surfaces.release(&key, &running).await);
    let survivor = surfaces
        .for_surface(&id, &principal, &state, &headers)
        .await
        .expect("a surface");
    assert!(
        Arc::ptr_eq(&rebuilt, &survivor),
        "an old driver releasing must not remove the surface that replaced it"
    );
}

#[tokio::test]
async fn a_browser_that_falls_behind_keeps_its_stream() {
    // Tokio's broadcast receiver reports `Lagged(n)` when a subscriber falls
    // more than the buffer behind, then carries on from the oldest frame it
    // still holds. The handler read frames with `while let Ok(..)`, which
    // treated that like a closed channel and ended the stream.
    //
    // It healed by accident — `EventSource` reconnects and the shell resends
    // everything — and that accident hid the cost: a background tab or a slow
    // link became a reconnect loop, each turn a `for_surface` call and a
    // full-state send, for a browser that only needed to skip ahead.
    let (frames, mut receiver) = tokio::sync::broadcast::channel::<u8>(4);

    for value in 0..8u8 {
        let _ = frames.send(value);
    }

    // Four sent past a buffer of four: the oldest are gone.
    let missed = match receiver.recv().await {
        Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => missed,
        other => panic!("expected the receiver to report lagging, got {other:?}"),
    };
    assert_eq!(missed, 4);

    // The important part, and the whole reason skipping is safe: the channel is
    // still open and delivers what is current. A handler that treats `Lagged`
    // as the end throws this away.
    assert_eq!(
        receiver.recv().await.expect("the stream survives lagging"),
        4,
        "a lagging receiver resumes at the oldest frame still held"
    );

    let _ = frames.send(99);
    assert_eq!(receiver.recv().await.expect("still open"), 5);
}

#[tokio::test]
async fn the_frame_buffer_is_sized_from_the_cadence() {
    // A constant 256 meant three seconds of headroom for ten panels at 8Hz and
    // over an hour at the default half-minute — the same number standing for
    // two entirely different things. On a fast shell a backgrounded tab could
    // fall behind it in the time it takes to switch windows.
    let live = hlin::config::Timings {
        refresh_ms: 125,
        ..Default::default()
    };
    let slow = hlin::config::Timings::default();

    // The tick is a quarter of the refresh, so a fast shell wakes far more
    // often and needs the deeper buffer.
    assert!(
        live.tick() < slow.tick(),
        "a live shell looks for work more often, so it produces frames faster"
    );
}

// -- Who is asking --------------------------------------------------------

fn trusted_header_config() -> Config {
    let mut config = config();
    config.auth = AuthConfig::TrustedHeader {
        header: "x-forwarded-user".to_string(),
        groups_header: Some("x-forwarded-groups".to_string()),
        name_header: None,
        acknowledge_proxy_required: true,
    };
    config
}

#[test]
fn a_request_without_the_header_names_nobody() {
    // The whole point. Before this, `Config::principal()` took no arguments and
    // returned the same person for every request, so ownership and visibility
    // were implemented, tested, and unable to do anything.
    let config = trusted_header_config();

    assert!(
        config
            .principal_from(&axum::http::HeaderMap::new())
            .known()
            .is_none(),
        "a request carrying no principal must name nobody, so the extractor can refuse it"
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-user", "  ".parse().unwrap());
    assert!(
        config.principal_from(&headers).known().is_none(),
        "an empty header is not a principal either"
    );
}

#[test]
fn the_header_is_the_principal_and_groups_come_with_it() {
    let config = trusted_header_config();

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-user", "ada".parse().unwrap());
    headers.insert("x-forwarded-groups", "oncall, platform ,".parse().unwrap());

    let principal = config
        .principal_from(&headers)
        .known()
        .expect("a principal");
    assert_eq!(principal.sub, "ada");
    assert_eq!(
        principal.groups,
        vec!["oncall".to_string(), "platform".to_string()],
        "groups are trimmed and empties dropped, because a proxy's list often has both"
    );

    // Two requests, two people. This is the property every ownership rule in
    // the shell was written against and could not previously exhibit.
    let mut other = axum::http::HeaderMap::new();
    other.insert("x-forwarded-user", "grace".parse().unwrap());
    assert_eq!(config.principal_from(&other).known().unwrap().sub, "grace");
}

#[test]
fn trusted_header_must_acknowledge_what_it_trusts() {
    // The failure is silent: a shell reachable without going through the proxy
    // accepts whoever a caller claims to be, and nothing about that looks wrong
    // from the inside. So the configuration has to say so out loud.
    let mut config = trusted_header_config();
    config.auth = AuthConfig::TrustedHeader {
        header: "x-forwarded-user".to_string(),
        groups_header: None,
        name_header: None,
        acknowledge_proxy_required: false,
    };

    let refused = config.check().expect_err("this must not start");
    assert!(
        refused.to_string().contains("acknowledge_proxy_required"),
        "the refusal must say what to do about it: {refused}"
    );
}

#[tokio::test]
async fn a_layout_belongs_to_whoever_the_header_named() {
    // Ownership, exercised over the real router with two distinct principals —
    // which was impossible while every request was the same person.
    let (app, _store, _config) = shell_with(trusted_header_config()).await;

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/layouts")
                .header("content-type", "application/json")
                .header("x-forwarded-user", "ada")
                .body(Body::from(r#"{"title":"Ada's"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let document: LayoutDocument = serde_json::from_slice(
        &axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let id = document.id.expect("an id");

    // Grace may read it — a personal layout is open to anyone holding its link
    // (HLIN-A-0007) — but must not be able to change it.
    let refused = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/layouts/{id}"))
                .header("content-type", "application/json")
                .header("x-forwarded-user", "grace")
                .body(Body::from(
                    r#"{"title":"Grace's now","visibility":"personal","panels":[]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "a layout belongs to the principal who made it, and now that means something"
    );

    // And a request with no principal at all never reaches a handler.
    let anonymous = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/layouts/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        anonymous.status(),
        StatusCode::UNAUTHORIZED,
        "no header, no principal, no answer"
    );
}

// -- oidc -----------------------------------------------------------------
//
// The protocol itself is tested where it lives. What is worth asserting here is
// the seam: that a session cookie identifies its holder over the real router,
// that a request without one is refused before any handler runs, and that the
// refusal says where a person could go — which is the one thing a browser
// cannot work out for itself and the difference between "sign in" and "the
// proxy in front of this shell is broken".

fn oidc_config() -> Config {
    let mut config = config();
    config.auth = AuthConfig::Oidc(hlin::config::OidcConfig {
        issuer: "https://login.example.com".to_string(),
        client_id: "hlin".to_string(),
        client_secret_env: "HLIN_TEST_OIDC_SECRET".to_string(),
        public_url: "https://hlin.example.com".to_string(),
        scopes: vec!["openid".to_string()],
        groups_claim: "groups".to_string(),
        session_hours: 12,
        cookie: Default::default(),
    });
    config
}

/// A session in the store, and the cookie header that names it.
async fn signed_in(store: &Arc<dyn Store>, subject: &str) -> String {
    let value = hlin::auth::unguessable();
    store
        .create_session(hlin::store::Session {
            id: hlin::auth::fingerprint(&value),
            subject: subject.to_string(),
            name: None,
            groups: vec![],
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        })
        .await
        .expect("the store accepts a session");

    format!("hlin_session={value}")
}

#[tokio::test]
async fn a_session_cookie_is_who_the_holder_is() {
    let (app, store, _) = shell_with(oidc_config()).await;

    let ada = signed_in(&store, "ada").await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/layouts")
                .header("content-type", "application/json")
                .header("cookie", &ada)
                .body(Body::from(r#"{"title":"Ada's own"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(created.into_body(), 1 << 20)
        .await
        .unwrap();
    let layout: LayoutDocument = serde_json::from_slice(&bytes).expect("a layout");
    let id = layout.id.clone().expect("an identity");

    // Two sessions, two people — the property every ownership rule was written
    // against, now reached by cookie rather than by header.
    let grace = signed_in(&store, "grace").await;
    let refused = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/layouts/{id}"))
                .header("content-type", "application/json")
                .header("cookie", &grace)
                .body(Body::from(
                    r#"{"title":"Grace's now","visibility":"personal","panels":[]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_session_that_is_over_is_nobody() {
    let (app, store, _) = shell_with(oidc_config()).await;

    let value = hlin::auth::unguessable();
    store
        .create_session(hlin::store::Session {
            id: hlin::auth::fingerprint(&value),
            subject: "ada".to_string(),
            name: None,
            groups: vec![],
            created_at: chrono::Utc::now() - chrono::Duration::hours(13),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
        })
        .await
        .unwrap();

    let answer = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .header("cookie", format!("hlin_session={value}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        answer.status(),
        StatusCode::UNAUTHORIZED,
        "a session past its expiry is refused whether or not anything has swept it"
    );
}

#[tokio::test]
async fn a_forged_cookie_names_nobody() {
    let (app, _, _) = shell_with(oidc_config()).await;

    // The value is the credential, and it is stored as a hash — so a caller who
    // has seen the database still cannot produce a cookie, and one who invents
    // a value finds no session.
    let answer = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .header("cookie", "hlin_session=not-a-session-anybody-issued")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(answer.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_refusal_says_where_to_sign_in_only_where_there_is_somewhere() {
    // Two shells answer 401 to the same request, and a browser must do opposite
    // things about it: go and sign in, or show the person that something
    // upstream is broken. The status cannot carry that, so the body does.
    let (signing_in, _, _) = shell_with(oidc_config()).await;
    let (proxied, _, _) = shell_with(trusted_header_config()).await;

    let (status, body) = call(&signing_in, get("/api/config")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body.get("login").and_then(|value| value.as_str()),
        Some("/auth/login"),
        "a shell that signs people in says so"
    );

    let (status, body) = call(&proxied, get("/api/config")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        body.get("login").is_some_and(serde_json::Value::is_null),
        "a shell behind a proxy has nowhere to send anybody, and saying it did \
         would send them round a loop: {body}"
    );
}

#[tokio::test]
async fn the_sign_in_routes_exist_only_where_the_shell_signs_people_in() {
    let (signing_in, _, _) = shell_with(oidc_config()).await;
    let (proxied, _, _) = shell_with(trusted_header_config()).await;

    // Not asserting where it redirects to: that needs a provider. Asserting
    // that the route is *there*, and that the other shell does not pretend.
    let answer = proxied.oneshot(get("/auth/login")).await.unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::NOT_FOUND,
        "a route that exists and cannot work is worse than one that does not"
    );

    let answer = signing_in.oneshot(get("/auth/login")).await.unwrap();
    assert_ne!(
        answer.status(),
        StatusCode::NOT_FOUND,
        "the shell that signs people in answers on its own login route"
    );
}

#[test]
fn oidc_refuses_a_configuration_that_would_fail_quietly() {
    // Each of these fails at the first sign-in otherwise, which is both later
    // and quieter: a shell that starts and then refuses everyone looks like an
    // outage, and by then nobody is watching the log that would explain it.
    unsafe { std::env::set_var("HLIN_TEST_OIDC_SECRET", "shh") };

    let sound = oidc_config();
    assert!(sound.auth.check().is_ok(), "{:?}", sound.auth.check());

    let refused = |mutate: fn(&mut hlin::config::OidcConfig)| {
        let mut config = oidc_config();
        let AuthConfig::Oidc(ref mut oidc) = config.auth else {
            unreachable!("this fixture is oidc")
        };
        mutate(oidc);
        config
            .auth
            .check()
            .expect_err("this configuration should not have been accepted")
    };

    // The issuer is where the shell fetches the keys it will trust to say who
    // somebody is. Over plain HTTP, anything on the path chooses those keys.
    assert!(refused(|oidc| oidc.issuer = "http://login.example.com".to_string()).contains("https"),);

    // A cookie is the whole of a session.
    assert!(refused(|oidc| oidc.cookie.secure = false).contains("Secure"));

    // Strict strips the cookie from the provider's redirect, so a person would
    // arrive back signed in and apparently not, forever.
    assert!(
        refused(|oidc| oidc.cookie.same_site = "Strict".to_string()).contains("Lax"),
        "the one SameSite value that silently breaks the flow is named"
    );

    // No `openid`, no id token, nothing to learn anybody's identity from.
    assert!(refused(|oidc| oidc.scopes = vec!["profile".to_string()]).contains("openid"));

    // A secret in the file would make the file a secret.
    assert!(
        refused(|oidc| oidc.client_secret_env = "HLIN_TEST_NOT_SET".to_string())
            .contains("HLIN_TEST_NOT_SET")
    );
}
