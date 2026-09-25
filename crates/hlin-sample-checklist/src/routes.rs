//! The HTTP surface: a manifest, a JSON API for the platform's module, the
//! shell-drawn panel's data, and an event stream.
//!
//! Every route but the manifest and health check verifies identity, and every
//! write verifies it *for that request*: [`HlinRequestIdentity`] refuses a
//! write whose token is not bound to its method and path, so a read token
//! lifted from a log cannot be spent on a change ([[HLIN-I-0010]] decision 6).
//! Reads take [`HlinIdentity`], which is what a read has always needed.
//!
//! What a verified caller may then do is decided in [`crate::lists`], never
//! here and never by the shell.

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::Utc;
use hlin_identity::extract::{FromRef, HlinIdentity, HlinRequestIdentity, IdentityState};
use hlin_manifest::envelope::{Choice, Column, ColumnType, Envelope, Options, Records};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::changes::{self, Changes};
use crate::idempotency::{Answer, Idempotency, REMEMBERED, Recall};
use crate::lists::{Caller, Item, List, Lists, Refused};
use crate::module::ModuleFiles;

/// The header a write's retry key arrives in.
pub const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// Set on an answer given from memory rather than by doing the write, so
/// whoever is debugging a retry can see which one they got.
pub const REPLAYED: &str = "idempotent-replayed";

/// The longest idempotency key accepted. Keys are opaque, but unbounded ones
/// would make the memory's bound a bound on count and not on size.
const MAX_KEY: usize = 255;

/// Everything a request handler can reach.
///
/// Cheap to clone: the state behind it is shared, so a test can keep a clone
/// and watch the same platform its requests are changing.
#[derive(Clone)]
pub struct App {
    name: String,
    identity: IdentityState,
    store: Arc<Mutex<Store>>,
    changes: Arc<Changes>,
    module: ModuleFiles,
}

/// The lists and the recent write keys, behind one lock.
///
/// One lock rather than two because checking a key, doing the write and
/// remembering its answer have to happen as one step. Two concurrent retries
/// of the same toggle would otherwise both find the key unseen and both flip
/// the item, which is the double write the key exists to prevent.
#[derive(Debug)]
struct Store {
    lists: Lists,
    remembered: Idempotency,
}

impl FromRef<App> for IdentityState {
    fn from_ref(app: &App) -> Self {
        app.identity.clone()
    }
}

impl FromRef<App> for ModuleFiles {
    fn from_ref(app: &App) -> Self {
        app.module.clone()
    }
}

impl App {
    /// A platform called `name`, verifying callers with `identity`, starting
    /// from these lists.
    pub fn new(name: impl Into<String>, identity: IdentityState, lists: Lists) -> Self {
        Self {
            name: name.into(),
            identity,
            store: Arc::new(Mutex::new(Store {
                lists,
                remembered: Idempotency::new(REMEMBERED),
            })),
            changes: Arc::new(Changes::new()),
            module: ModuleFiles::none(),
        }
    }

    /// Serve this built module under the `assets` prefix. Without one, the
    /// module's files are 404 and the shell draws the `items` panel as its
    /// `table` fallback.
    pub fn with_module(mut self, module: ModuleFiles) -> Self {
        self.module = module;
        self
    }

    /// Where this platform announces changes.
    pub fn changes(&self) -> &Changes {
        &self.changes
    }

    /// A copy of the lists as they are now, for the tests.
    pub fn lists(&self) -> Lists {
        self.store
            .lock()
            .expect("the store lock is not poisoned")
            .lists
            .clone()
    }
}

/// The router for this platform.
///
/// The module's API is resource-shaped, under `/api/lists/`. Toggling is its
/// own `POST` rather than a `PATCH` of `done`, because "cross this off" is what
/// a person does and what any member may do, while a `PATCH` is an edit and
/// only an author or owner may make one. Keeping them on different routes
/// keeps the two rules from sharing a handler.
pub fn router(app: App) -> Router {
    Router::new()
        .route("/.well-known/hlin.json", get(manifest))
        .route("/api/health", get(health))
        .route("/api/lists", get(lists))
        .route("/api/lists/{list}/items", get(items).post(add))
        .route("/api/lists/{list}/items/{item}", patch(edit).delete(delete))
        .route("/api/lists/{list}/items/{item}/toggle", post(toggle))
        .route("/api/hlin/items", get(panel_items))
        .route("/api/hlin/lists", get(panel_lists))
        .route("/api/events", get(events))
        .route(
            &format!("{}{{file}}", crate::module::ITEMS_DIR),
            get(crate::module::asset),
        )
        .with_state(app)
}

/// The manifest, served to anyone, as in `hlin-sample-platform`.
async fn manifest(State(app): State<App>) -> Json<Value> {
    Json(serde_json::to_value(crate::manifest::build(&app.name)).expect("the manifest serialises"))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

// -- Reads -------------------------------------------------------------------

/// The lists the caller belongs to, for the module's picker.
async fn lists(State(app): State<App>, HlinIdentity(claims): HlinIdentity) -> Response {
    let caller = Caller::from_claims(&claims);
    let store = app.store.lock().expect("the store lock is not poisoned");
    let lists: Vec<&List> = store.lists.belonging_to(&caller);
    Json(json!({ "lists": lists })).into_response()
}

/// One list and its items.
///
/// Says who the viewer is to this list, which is the platform talking to its
/// own module and not to the shell: the module may use it to hide an edit the
/// platform would refuse ([[HLIN-I-0010]] decision 5), and the platform
/// refuses it regardless.
async fn items(
    State(app): State<App>,
    HlinIdentity(claims): HlinIdentity,
    Path(list): Path<String>,
) -> Response {
    let caller = Caller::from_claims(&claims);
    let store = app.store.lock().expect("the store lock is not poisoned");
    match store.lists.read(&caller, &list) {
        Ok(found) => Json(json!({
            "list": found,
            "viewer": { "id": caller.id, "owner": found.is_owned_by(&caller) },
            "items": found.items,
        }))
        .into_response(),
        Err(refused) => refusal(&refused),
    }
}

// -- Writes ------------------------------------------------------------------

#[derive(Deserialize)]
struct Text {
    text: String,
}

/// Add an item. `201` with the item.
async fn add(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path(list): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let write = Write::new(&claims, &method, &uri, &headers, &body);
    write.apply(&app, |lists, caller| {
        lists.read(caller, &list)?;
        let text = text_in(&body)?;
        let item = lists.add(caller, &list, &text)?;
        Ok(Done::item(StatusCode::CREATED, &list, &item))
    })
}

/// Cross an item off, or back on. `200` with the item.
async fn toggle(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path((list, item)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let write = Write::new(&claims, &method, &uri, &headers, &body);
    write.apply(&app, |lists, caller| {
        let item = lists.toggle(caller, &list, &item)?;
        Ok(Done::item(StatusCode::OK, &list, &item))
    })
}

/// Change an item's text. `200` with the item.
async fn edit(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path((list, item)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let write = Write::new(&claims, &method, &uri, &headers, &body);
    write.apply(&app, |lists, caller| {
        lists.read(caller, &list)?;
        let text = text_in(&body)?;
        let item = lists.edit(caller, &list, &item, &text)?;
        Ok(Done::item(StatusCode::OK, &list, &item))
    })
}

/// Remove an item. `204`.
async fn delete(
    State(app): State<App>,
    HlinRequestIdentity(claims): HlinRequestIdentity,
    Path((list, item)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let write = Write::new(&claims, &method, &uri, &headers, &body);
    write.apply(&app, |lists, caller| {
        lists.delete(caller, &list, &item)?;
        Ok(Done {
            status: StatusCode::NO_CONTENT,
            body: None,
            list: list.clone(),
        })
    })
}

fn text_in(body: &[u8]) -> Result<String, Refused> {
    serde_json::from_slice::<Text>(body)
        .map(|parsed| parsed.text)
        .map_err(|_| Refused::Invalid("Send the item as JSON with a `text` field."))
}

/// A write that succeeded: what to answer, and which list to announce.
struct Done {
    status: StatusCode,
    body: Option<Value>,
    list: String,
}

impl Done {
    fn item(status: StatusCode, list: &str, item: &Item) -> Self {
        Self {
            status,
            body: Some(serde_json::to_value(item).expect("an item serialises")),
            list: list.to_string(),
        }
    }
}

/// One write, as the platform sees it before doing anything.
struct Write<'a> {
    caller: Caller,
    key: Result<&'a str, Refused>,
    /// What identifies this write for its key: method, path and body. A key
    /// sent again with any of these different is a different write.
    request: String,
}

impl<'a> Write<'a> {
    fn new(
        claims: &hlin_identity::Claims,
        method: &Method,
        uri: &Uri,
        headers: &'a HeaderMap,
        body: &[u8],
    ) -> Self {
        Self {
            caller: Caller::from_claims(claims),
            key: idempotency_key(headers),
            request: format!("{method} {}\n{}", uri.path(), String::from_utf8_lossy(body)),
        }
    }

    /// Do the write once, however many times it is asked for.
    ///
    /// The key is checked, the change made and its answer remembered under
    /// one lock, and the change announced only after the lock is released and
    /// the write has landed.
    fn apply(
        self,
        app: &App,
        change: impl FnOnce(&mut Lists, &Caller) -> Result<Done, Refused>,
    ) -> Response {
        let key = match self.key {
            Ok(key) => key,
            Err(refused) => return refusal(&refused),
        };

        let (answer, changed) = {
            let mut store = app.store.lock().expect("the store lock is not poisoned");

            match store.remembered.recall(&self.caller.id, key, &self.request) {
                Recall::Fresh => {}
                Recall::Replay(answer) => return respond(answer, true),
                Recall::Reused => {
                    return respond(
                        Answer {
                            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                            body: Some(message(
                                "This idempotency key was already used for a different change.",
                            )),
                        },
                        false,
                    );
                }
            }

            let (answer, changed) = match change(&mut store.lists, &self.caller) {
                Ok(done) => (
                    Answer {
                        status: done.status.as_u16(),
                        body: done.body,
                    },
                    Some(done.list),
                ),
                Err(refused) => (
                    Answer {
                        status: refused.status(),
                        body: Some(message(&refused.message())),
                    },
                    None,
                ),
            };

            store
                .remembered
                .remember(&self.caller.id, key, &self.request, answer.clone());
            (answer, changed)
        };

        if let Some(list) = changed {
            app.changes.list_changed(&list);
        }
        respond(answer, false)
    }
}

/// The write's key.
///
/// Required. The shell sends one with every write, and a toggle applied twice
/// undoes itself, so a write that cannot be told apart from its own retry is
/// one this platform will not make. A caller using curl sends a header, which
/// is a small price for a list that stays the way people left it. The feed
/// requires one for the same reason, so the two reference platforms teach
/// one rule.
fn idempotency_key(headers: &HeaderMap) -> Result<&str, Refused> {
    let Some(value) = headers.get(IDEMPOTENCY_KEY) else {
        return Err(Refused::Invalid(
            "Every change needs an Idempotency-Key header, so a retry is never applied twice.",
        ));
    };
    match value.to_str() {
        Ok(key) if !key.is_empty() && key.len() <= MAX_KEY => Ok(key),
        _ => Err(Refused::Invalid(
            "The Idempotency-Key header must be between 1 and 255 visible characters.",
        )),
    }
}

fn respond(answer: Answer, replayed: bool) -> Response {
    let status = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = match answer.body {
        Some(body) => (status, Json(body)).into_response(),
        None => status.into_response(),
    };
    if replayed {
        response
            .headers_mut()
            .insert(REPLAYED, HeaderValue::from_static("true"));
    }
    response
}

fn message(text: &str) -> Value {
    json!({ "message": text })
}

/// A refusal, in the platform's own words.
fn refusal(refused: &Refused) -> Response {
    respond(
        Answer {
            status: refused.status(),
            body: Some(message(&refused.message())),
        },
        false,
    )
}

// -- The shell-drawn panel ---------------------------------------------------

#[derive(Deserialize, Default)]
struct PanelParams {
    list: Option<String>,
}

/// `records.v1`: one list's items, for the shell to draw as a table.
///
/// With no list chosen, the viewer's first list, as the parameter vocabulary
/// asks: a panel just placed on a surface has selected nothing, and a list is
/// a better first sight than an error. With a list chosen that the viewer is
/// not on, the same refusal the module would get, which the shell shows as
/// `forbidden`.
async fn panel_items(
    State(app): State<App>,
    HlinIdentity(claims): HlinIdentity,
    Query(params): Query<PanelParams>,
) -> Response {
    let caller = Caller::from_claims(&claims);
    let store = app.store.lock().expect("the store lock is not poisoned");

    let items: Vec<Item> = match params.list.as_deref() {
        Some(list) => match store.lists.read(&caller, list) {
            Ok(found) => found.items.clone(),
            Err(refused) => return refusal(&refused),
        },
        None => store
            .lists
            .belonging_to(&caller)
            .first()
            .map(|list| list.items.clone())
            .unwrap_or_default(),
    };

    let rows = items
        .into_iter()
        .map(|item| {
            let mut row = Map::new();
            row.insert("id".to_string(), Value::String(item.id));
            row.insert("done".to_string(), Value::Bool(item.done));
            row.insert("text".to_string(), Value::String(item.text));
            row.insert("author".to_string(), Value::String(item.author));
            row
        })
        .collect();

    envelope(Envelope::Records(Records {
        columns: vec![
            column("done", "Done", ColumnType::Boolean),
            column("text", "Item", ColumnType::String),
            column("author", "Added by", ColumnType::String),
        ],
        rows,
        as_of: Some(Utc::now()),
        extra: Default::default(),
    }))
}

/// `options.v1`: the lists the viewer belongs to.
async fn panel_lists(State(app): State<App>, HlinIdentity(claims): HlinIdentity) -> Response {
    let caller = Caller::from_claims(&claims);
    let store = app.store.lock().expect("the store lock is not poisoned");
    let options = store
        .lists
        .belonging_to(&caller)
        .into_iter()
        .map(|list| Choice {
            value: list.id.clone(),
            label: list.name.clone(),
            group: None,
            extra: Default::default(),
        })
        .collect();

    envelope(Envelope::Options(Options {
        options,
        as_of: Some(Utc::now()),
        extra: Default::default(),
    }))
}

fn column(key: &str, label: &str, value_type: ColumnType) -> Column {
    Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    }
}

fn envelope(document: Envelope) -> Response {
    Json(serde_json::to_value(document).expect("an envelope serialises")).into_response()
}

// -- The event stream --------------------------------------------------------

/// Which lists have just changed, as `changed` events for the `items` panel.
///
/// Behind identity like everything else, for the sample platform's reason: a
/// stream of which lists are changing says something about who is working,
/// and there is no reason for it to be the one thing anybody can read.
async fn events(State(app): State<App>, HlinIdentity(_): HlinIdentity) -> Response {
    // Subscribed before the response is returned, so a change between now
    // and the first poll of the stream is queued rather than missed.
    let mut changed = app.changes.subscribe();

    let stream = async_stream::stream! {
        loop {
            let data = match changed.recv().await {
                Ok(list) => changes::event_data(Some(&list)),
                // Which lists were missed is exactly what is not known, so
                // the panel is announced with no selection: every instance
                // refetches, which is true and cheap.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::debug!(missed, "a subscriber fell behind");
                    changes::event_data(None)
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            yield Ok::<_, std::convert::Infallible>(
                axum::response::sse::Event::default()
                    .event("changed")
                    .data(data.to_string()),
            );
        }
    };

    // The heartbeat is required by HLIN-S-0006: without it a connection that
    // stopped delivering without closing would look live to the shell forever.
    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response()
}
