//! The running platform: a widget's state, who is asking, and the routes every
//! widget has.
//!
//! A widget's own routes are handlers on [`Platform<S>`], where `S` is its
//! state. They take [`Viewer`] to read and [`Write`] to change anything, and
//! answer through [`Platform::read`] and [`Platform::write`]. [`router`] adds
//! the routes no widget should have to write: the manifest, health, the event
//! stream, the fallback's data and the module's files.
//!
//! What [`Platform::write`] does for every write, in order, so no widget can
//! forget one:
//!
//! 1. The token was checked before the handler ran, and bound to this method
//!    and path (the [`Write`] extractor, over `hlin-identity`), so a read token
//!    lifted from a log cannot be spent on a write.
//! 2. The `Idempotency-Key` is checked against what this person already did.
//!    A repeat is answered as it was the first time, marked
//!    `Idempotency-Replayed`; the same key on a different request is a 422.
//! 3. The widget's own rule runs, under the one lock that also holds the keys,
//!    so the same key arriving twice at once cannot act twice.
//! 4. A write that happened is remembered, the lock released, and then the
//!    event stream is told, so a shell that refetches the moment it hears
//!    cannot read the old state.
//!
//! Refusals are `{ "message" }`, written for the person who will read them:
//! the shell passes them to the module unchanged, and the module shows them as
//! they came ([`Refusal`]).

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{FromRef, FromRequest, FromRequestParts, Path, Query, Request, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hlin_identity::extract::{HlinRequestIdentity, IdentityState};
use hlin_identity::{Claims, Verifier};
use hlin_manifest::envelope::Envelope;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::files::ModuleFiles;
use crate::idempotency::{self, Answer, Fingerprint, Remembered, Seen};
use crate::widget::Widget;

/// A verified viewer, for a read: `Viewer(claims)` as a handler argument.
///
/// `hlin-identity`'s request extractor under a shorter name. On a read it
/// accepts the shell's ordinary token; a handler cannot run without one.
pub use hlin_identity::extract::HlinRequestIdentity as Viewer;

/// The header a write's idempotency key arrives in.
pub const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// The largest write body a widget reads, in bytes.
///
/// Widgets take small JSON objects. The shell has its own limit, far larger;
/// this one is the widget's own, so a widget run with no shell in front of it
/// is not a place to park a gigabyte.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// A widget, running: its description, its state, and who to tell of changes.
pub struct Platform<S> {
    inner: Arc<Inner<S>>,
}

impl<S> Clone for Platform<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct Inner<S> {
    id: String,
    widget: Widget<S>,
    identity: IdentityState,
    /// The widget's state and the keys acted on, behind one lock, so checking
    /// a key and acting on it cannot interleave with the same key arriving
    /// twice at once.
    held: Mutex<Held<S>>,
    /// Tells every open event stream that something changed.
    told: broadcast::Sender<()>,
    module: ModuleFiles,
}

struct Held<S> {
    state: S,
    remembered: Remembered,
}

impl<S> FromRef<Platform<S>> for IdentityState {
    fn from_ref(platform: &Platform<S>) -> Self {
        platform.inner.identity.clone()
    }
}

impl<S: Send + 'static> Platform<S> {
    /// A widget running as the platform called `id`, starting from `state`,
    /// trusting tokens `verifier` accepts, and serving `module`.
    pub fn new(
        id: impl Into<String>,
        widget: Widget<S>,
        state: S,
        verifier: Arc<Verifier>,
        module: ModuleFiles,
    ) -> Self {
        let id = id.into();
        // Sixty-four notices of backlog is generous: a subscriber that falls
        // behind is told once that something changed, which is true and enough.
        let (told, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(Inner {
                identity: IdentityState {
                    verifier,
                    audience: id.clone(),
                },
                id,
                widget,
                held: Mutex::new(Held {
                    state,
                    remembered: Remembered::default(),
                }),
                told,
                module,
            }),
        }
    }

    /// The platform's id: its `platform.id`, and the audience every token
    /// must name.
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// The widget this is.
    pub fn widget(&self) -> &Widget<S> {
        &self.inner.widget
    }

    /// Look at the state. For a read.
    pub fn read<T>(&self, look: impl FnOnce(&S) -> T) -> T {
        look(&self.inner.held.lock().expect("not poisoned").state)
    }

    /// Make a write, once: the key, the lock, the rule, the memory of what was
    /// done, and telling the event stream (see the module docs for the order).
    ///
    /// `act` is the widget's own rule. It decides from the state and the
    /// person asking, changes the state if it allows the write, and says what
    /// to answer. A refusal changes nothing and is not remembered, so a retry
    /// is decided again.
    pub fn write(
        &self,
        write: &Write,
        act: impl FnOnce(&mut S, &Claims) -> Result<Reply, Refusal>,
    ) -> Response {
        let request = Fingerprint::new(write.method.as_str(), &write.path, &write.body);
        let who = &write.claims.sub;
        let mut held = self.inner.held.lock().expect("not poisoned");

        match held.remembered.check(who, &write.key, &request) {
            Seen::Again(answer) => return respond(answer, true),
            Seen::Conflict => {
                return Refusal::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "This Idempotency-Key was already used for a different request",
                )
                .into_response();
            }
            Seen::New => {}
        }

        match act(&mut held.state, &write.claims) {
            Ok(reply) => {
                let answer = reply.into_answer();
                held.remembered
                    .remember(who, &write.key, request, answer.clone());
                drop(held);
                self.announce();
                respond(answer, false)
            }
            Err(refusal) => refusal.into_response(),
        }
    }

    /// Tell every open event stream that this widget's panel changed.
    ///
    /// [`Platform::write`] calls this after every write that happened. Call it
    /// yourself only for a change nobody wrote: something the widget's own
    /// clock did.
    pub fn announce(&self) {
        // Fails only when nobody is subscribed, which is not worth a word.
        let _ = self.inner.told.send(());
    }

    /// How many event streams are open, for a test that must not announce
    /// before anybody is listening.
    pub fn listeners(&self) -> usize {
        self.inner.told.receiver_count()
    }
}

/// The router for a running widget: its own `api`, plus everything every
/// widget serves.
///
/// | Method | Path | What |
/// |---|---|---|
/// | `GET` | `/.well-known/hlin.json` | The manifest, to anyone |
/// | `GET` | `/api/health` | Health, to anyone |
/// | `GET` | `/api/events` | `changed` after every write, to a signed-in viewer |
/// | `GET` | `/api/panels/{panel}` | The fallback's envelope, where there is a fallback |
/// | `GET` | `/ui/{panel}/{file}` | The module's files, to anyone: the shell fetches them as itself |
pub fn router<S: Send + 'static>(platform: Platform<S>, api: Router<Platform<S>>) -> Router {
    let widget = platform.widget();
    let mut router = api
        .route("/.well-known/hlin.json", get(manifest::<S>))
        .route("/api/health", get(health))
        .route("/api/events", get(events::<S>))
        .route(&format!("{}{{file}}", widget.module_dir()), get(asset::<S>));
    if widget.fallback.is_some() {
        router = router.route(&format!("/{}", widget.fallback_data()), get(fallback::<S>));
    }
    router.with_state(platform)
}

/// The manifest, served to anyone: the shell fetches it as itself.
async fn manifest<S: Send + 'static>(State(platform): State<Platform<S>>) -> Json<Value> {
    Json(
        serde_json::to_value(platform.widget().manifest(platform.id()))
            .expect("the manifest serialises"),
    )
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn asset<S: Send + 'static>(
    State(platform): State<Platform<S>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    platform.inner.module.serve(&name, &headers)
}

/// The fallback's data, as the person asking would see it.
///
/// A series is cut to the time range the shell asked for, where it asked for
/// one (`from` and `to`, [[HLIN-S-0002]]); `step` is a hint, and not taken.
async fn fallback<S: Send + 'static>(
    State(platform): State<Platform<S>>,
    Viewer(claims): Viewer,
    Query(range): Query<Range>,
) -> Response {
    let Some(fallback) = platform.widget().fallback.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut envelope = platform.read(|state| (fallback.data)(state, &claims));
    if let Envelope::Series(series) = &mut envelope {
        for line in &mut series.series {
            line.points.retain(|point| range.holds(point.at()));
        }
    }
    Json(serde_json::to_value(envelope).expect("an envelope serialises")).into_response()
}

/// The time range on a fallback's request, where the panel declares one.
#[derive(Debug, Default, serde::Deserialize)]
struct Range {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

impl Range {
    /// Whether an instant, in epoch milliseconds, is inside: from inclusive,
    /// to exclusive, as the bridge's `TimeRange` is.
    fn holds(&self, at: i64) -> bool {
        self.from.is_none_or(|from| at >= from.timestamp_millis())
            && self.to.is_none_or(|to| at < to.timestamp_millis())
    }
}

/// `changed` for the panel, whenever anything changes.
///
/// News, not data ([[HLIN-S-0006]]): what the panel holds may depend on who is
/// asking, and only a fetch made as them can say. Behind identity like every
/// other read.
async fn events<S: Send + 'static>(
    State(platform): State<Platform<S>>,
    Viewer(_): Viewer,
) -> Response {
    // Subscribed before the response is returned, so a change between now and
    // the first poll of the stream is queued rather than missed.
    let mut changed = platform.inner.told.subscribe();
    let data = json!({ "panel": platform.widget().panel }).to_string();

    let stream = async_stream::stream! {
        // A subscriber that fell behind missed some notices, all about the one
        // panel, so telling it once is both true and enough. Only the sender
        // going away ends the stream.
        while let Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) = changed.recv().await {
            yield Ok::<_, std::convert::Infallible>(
                axum::response::sse::Event::default().event("changed").data(data.clone()),
            );
        }
    };

    // The heartbeat is required ([[HLIN-S-0006]]): without it a connection
    // that stopped delivering looks exactly like a quiet widget.
    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response()
}

// -- Writes -----------------------------------------------------------------

/// A write, as a handler receives it: who is asking, checked and bound to
/// this request, its idempotency key, and its body.
///
/// A handler taking this cannot run for a request without a bound token or
/// without an acceptable `Idempotency-Key`: both are refused before it, the
/// first with a 401 whose body says why, the second with a 400.
#[derive(Debug, Clone)]
pub struct Write {
    claims: Claims,
    method: Method,
    path: String,
    key: String,
    body: Bytes,
}

impl Write {
    /// Who is asking.
    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    /// The body as JSON of this shape, or a 400 in `message`'s words.
    pub fn json<T: DeserializeOwned>(&self, message: &str) -> Result<T, Refusal> {
        serde_json::from_slice(&self.body).map_err(|_| Refusal::bad_request(message))
    }
}

impl<S> FromRequest<S> for Write
where
    IdentityState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = request.into_parts();
        let HlinRequestIdentity(claims) =
            HlinRequestIdentity::from_request_parts(&mut parts, state).await?;

        let Some(key) = parts
            .headers
            .get(IDEMPOTENCY_KEY)
            .and_then(|value| value.to_str().ok())
        else {
            return Err(Refusal::bad_request("Writes need an Idempotency-Key").into_response());
        };
        if !idempotency::acceptable(key) {
            return Err(Refusal::bad_request(
                "An Idempotency-Key is up to 255 visible ASCII characters",
            )
            .into_response());
        }

        let body = axum::body::to_bytes(body, MAX_BODY_BYTES)
            .await
            .map_err(|_| {
                Refusal::new(StatusCode::PAYLOAD_TOO_LARGE, "That is too much to send")
                    .into_response()
            })?;

        Ok(Write {
            claims,
            method: parts.method,
            path: parts.uri.path().to_string(),
            key: key.to_string(),
            body,
        })
    }
}

/// What a write that happened answers.
#[derive(Debug, Clone)]
pub struct Reply {
    status: StatusCode,
    body: Option<Value>,
}

impl Reply {
    /// 200, with this body.
    pub fn ok(body: impl Serialize) -> Self {
        Self::with(StatusCode::OK, body)
    }

    /// 201, with this body.
    pub fn created(body: impl Serialize) -> Self {
        Self::with(StatusCode::CREATED, body)
    }

    /// 204, with nothing to say.
    pub fn no_content() -> Self {
        Self {
            status: StatusCode::NO_CONTENT,
            body: None,
        }
    }

    fn with(status: StatusCode, body: impl Serialize) -> Self {
        Self {
            status,
            body: Some(serde_json::to_value(body).expect("a reply serialises")),
        }
    }

    fn into_answer(self) -> Answer {
        Answer {
            status: self.status.as_u16(),
            body: self.body,
        }
    }
}

/// A refusal the widget decided, in its own words.
///
/// `message` rather than a code, because the module shows it to a person as
/// it is and the shell passes it on unchanged. A 401 is not one of these: a
/// refused credential is answered by the identity extractor, whose words are
/// for whoever is debugging the deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The status.
    pub status: StatusCode,
    /// What to tell the person.
    pub message: String,
}

impl Refusal {
    /// A refusal with this status and these words.
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// 400: the request itself is wrong.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// 403: this person may not.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// 404: there is no such thing.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// 409: allowed, but not in the state things are in.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "message": self.message }))).into_response()
    }
}

/// An answer, first time or again.
///
/// A repeat says so in `Idempotency-Replayed`, which nothing depends on but
/// which makes a retry visible to whoever is reading the traffic.
fn respond(answer: Answer, replayed: bool) -> Response {
    let status = StatusCode::from_u16(answer.status).expect("a status this widget chose");
    let mut response = match answer.body {
        Some(body) => (status, Json(body)).into_response(),
        None => status.into_response(),
    };
    if replayed {
        response
            .headers_mut()
            .insert("idempotency-replayed", HeaderValue::from_static("true"));
    }
    response
}
