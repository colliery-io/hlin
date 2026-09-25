//! The HTTP surface.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/.well-known/hlin.json` | The manifest, to anyone |
//! | `GET` | `/api/health` | Health, to anyone |
//! | `GET` | `/api/posts` | Every post, newest first |
//! | `POST` | `/api/posts` | Write a post: `{ "body" }` |
//! | `PUT` | `/api/posts/{id}` | Change what a post says: `{ "body" }` |
//! | `DELETE` | `/api/posts/{id}` | Take a post down |
//! | `GET` | `/api/panels/posts` | The posts as `records.v1`, for the shell's table |
//! | `GET` | `/api/events` | `changed` whenever a post does |
//!
//! Everything under `/api/` but health is behind identity. Reads accept the
//! shell's ordinary token; writes need one bound to their method and path, and
//! an `Idempotency-Key`. Refusals this platform decides are `{ "message" }`,
//! written for the person who will read them.

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use hlin_identity::Claims;
use hlin_identity::extract::{HlinRequestIdentity, IdentityState};
use serde_json::json;
use tokio::sync::broadcast;

use crate::idempotency::{self, Answer, Fingerprint, Remembered, Seen};
use crate::posts::{Draft, Post, Posts, Problem};
use crate::rules::Rules;

/// The header a write's idempotency key arrives in.
pub const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// How this platform was started.
pub struct Config {
    /// The platform's id: its `platform.id`, and the audience every token must
    /// name.
    pub name: String,
    /// Checks the shell's tokens.
    pub verifier: Arc<hlin_identity::Verifier>,
    /// Who may do what.
    pub rules: Rules,
    /// The posts to start with.
    pub posts: Vec<Post>,
}

/// What every handler shares.
#[derive(Clone)]
struct App {
    name: Arc<str>,
    identity: IdentityState,
    rules: Arc<Rules>,
    /// The posts and the keys acted on, behind one lock, so checking a key and
    /// acting on it cannot interleave with the same key arriving twice at once.
    feed: Arc<Mutex<Feed>>,
    /// Tells every open event stream that the posts changed.
    told: broadcast::Sender<()>,
}

struct Feed {
    posts: Posts,
    remembered: Remembered,
}

impl FromRef<App> for IdentityState {
    fn from_ref(app: &App) -> Self {
        app.identity.clone()
    }
}

/// The router for a feed with this configuration.
pub fn router(config: Config) -> Router {
    let identity = IdentityState {
        verifier: config.verifier,
        audience: config.name.clone(),
        // Somebody at the posting domain, so a developer running with no shell
        // can try every write.
        #[cfg(feature = "dev-identity")]
        development_principal: hlin_identity::Principal {
            sub: "developer".to_string(),
            name: Some("Developer".to_string()),
            email: Some(format!("developer@{}", config.rules.domain())),
            groups: vec![],
        },
    };

    // Sixty-four notices of backlog is generous: a subscriber that falls behind
    // is told once that something changed, which is true and enough.
    let (told, _) = broadcast::channel(64);

    let app = App {
        name: config.name.into(),
        identity,
        rules: Arc::new(config.rules),
        feed: Arc::new(Mutex::new(Feed {
            posts: Posts::new(config.posts),
            remembered: Remembered::default(),
        })),
        told,
    };

    Router::new()
        .route("/.well-known/hlin.json", get(manifest))
        .route("/api/health", get(health))
        .route("/api/posts", get(list).post(create))
        .route("/api/posts/{id}", axum::routing::put(edit).delete(delete))
        .route("/api/panels/posts", get(panel))
        .route("/api/events", get(events))
        .with_state(app)
}

/// The manifest, served to anyone: the shell fetches it as itself.
async fn manifest(State(app): State<App>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(crate::manifest::build(&app.name)).expect("the manifest serialises"))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Every post, newest first.
///
/// Wrapped in an object rather than a bare array so the answer can grow a
/// field without a module that reads it breaking.
async fn list(State(app): State<App>, HlinRequestIdentity(_): HlinRequestIdentity) -> Response {
    let posts = app
        .feed
        .lock()
        .expect("the feed is not poisoned")
        .posts
        .newest_first();
    Json(json!({ "posts": posts })).into_response()
}

/// The posts as a table, for the shell to draw where the module is not.
async fn panel(State(app): State<App>, HlinRequestIdentity(_): HlinRequestIdentity) -> Response {
    let envelope = app
        .feed
        .lock()
        .expect("the feed is not poisoned")
        .posts
        .as_records();
    Json(serde_json::to_value(envelope).expect("an envelope serialises")).into_response()
}

async fn create(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    write(
        &app,
        &claims,
        &method,
        &uri,
        &headers,
        &body,
        |posts, rules| {
            let draft = draft(&body)?;
            let post = posts.create(rules, &claims, &draft)?;
            Ok(answer(StatusCode::CREATED, Some(&post)))
        },
    )
}

async fn edit(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path(id): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    write(
        &app,
        &claims,
        &method,
        &uri,
        &headers,
        &body,
        |posts, rules| {
            let draft = draft(&body)?;
            let post = posts.edit(rules, &claims, &id, &draft)?;
            Ok(answer(StatusCode::OK, Some(&post)))
        },
    )
}

async fn delete(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path(id): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    write(
        &app,
        &claims,
        &method,
        &uri,
        &headers,
        &body,
        |posts, rules| {
            posts.delete(rules, &claims, &id)?;
            Ok(answer(StatusCode::NO_CONTENT, None))
        },
    )
}

/// Everything a write has in common: the key, the lock, the memory of what was
/// already done, and telling the event stream.
///
/// The token was checked, bound to this method and path, before the handler
/// ran; this is what comes after.
fn write(
    app: &App,
    claims: &Claims,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
    act: impl FnOnce(&mut Posts, &Rules) -> Result<Answer, Problem>,
) -> Response {
    let Some(key) = headers
        .get(IDEMPOTENCY_KEY)
        .and_then(|value| value.to_str().ok())
    else {
        return refuse(StatusCode::BAD_REQUEST, "Writes need an Idempotency-Key");
    };
    if !idempotency::acceptable(key) {
        return refuse(
            StatusCode::BAD_REQUEST,
            "An Idempotency-Key is up to 255 visible ASCII characters",
        );
    }

    let request = Fingerprint::new(method.as_str(), uri.path(), body);
    let mut feed = app.feed.lock().expect("the feed is not poisoned");

    match feed.remembered.check(&claims.sub, key, &request) {
        Seen::Again(answer) => return respond(answer, true),
        Seen::Conflict => {
            return refuse(
                StatusCode::UNPROCESSABLE_ENTITY,
                "This Idempotency-Key was already used for a different request",
            );
        }
        Seen::New => {}
    }

    match act(&mut feed.posts, &app.rules) {
        Ok(answer) => {
            feed.remembered
                .remember(&claims.sub, key, request, answer.clone());
            drop(feed);
            // After the lock is released and the write is visible, so a shell
            // that refetches the moment it hears cannot read the old posts.
            let _ = app.told.send(());
            respond(answer, false)
        }
        Err(problem) => {
            drop(feed);
            match problem {
                Problem::Invalid(message) => refuse(StatusCode::BAD_REQUEST, message),
                Problem::NotFound => refuse(StatusCode::NOT_FOUND, "There is no such post"),
                Problem::Forbidden(forbidden) => {
                    refuse(StatusCode::FORBIDDEN, &forbidden.to_string())
                }
            }
        }
    }
}

fn draft(body: &[u8]) -> Result<Draft, Problem> {
    serde_json::from_slice(body)
        .map_err(|_| Problem::Invalid("A post is a JSON object with a \"body\" string"))
}

fn answer(status: StatusCode, post: Option<&Post>) -> Answer {
    Answer {
        status: status.as_u16(),
        body: post.map(|post| serde_json::to_value(post).expect("a post serialises")),
    }
}

/// An answer, first time or again.
///
/// A repeat says so in `Idempotency-Replayed`, which nothing depends on but
/// which makes a retry visible to whoever is reading the traffic.
fn respond(answer: Answer, replayed: bool) -> Response {
    let status = StatusCode::from_u16(answer.status).expect("a status this platform chose");
    let mut response = match answer.body {
        Some(body) => (status, Json(body)).into_response(),
        None => status.into_response(),
    };
    if replayed {
        response.headers_mut().insert(
            "idempotency-replayed",
            axum::http::HeaderValue::from_static("true"),
        );
    }
    response
}

/// A refusal this platform decided, in its own words.
///
/// `message` rather than a code, because the module shows it to a person as
/// it is and the shell passes it on unchanged. A 401 is not one of these: a
/// refused credential is answered by the identity extractor, whose words are
/// for whoever is debugging the deployment.
fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}

/// `changed` for the posts panel, whenever a post changes.
///
/// The same shape as the read-only reference's stream ([[HLIN-S-0006]]):
/// news, not data. Behind identity like every other read here.
async fn events(State(app): State<App>, HlinRequestIdentity(_): HlinRequestIdentity) -> Response {
    // Subscribed before the response is returned, so a change between now and
    // the first poll of the stream is queued rather than missed.
    let mut changed = app.told.subscribe();

    let stream = async_stream::stream! {
        // A subscriber that fell behind missed some notices, all of them about
        // the one panel, so telling it once is both true and enough. Only the
        // sender going away ends the stream.
        while let Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) = changed.recv().await {
            yield Ok::<_, std::convert::Infallible>(
                axum::response::sse::Event::default()
                    .event("changed")
                    .data(json!({ "panel": crate::manifest::POSTS_PANEL }).to_string()),
            );
        }
    };

    // The heartbeat is required ([[HLIN-S-0006]]): without it a connection
    // that stopped delivering looks exactly like a quiet feed.
    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response()
}
