//! Talking to the shell about anything that is not the stream.
//!
//! Thin on purpose. Every function is one request and one deserialisation, so
//! the interesting decisions stay in `draft` and `state`, where they can be
//! tested without a browser.

use gloo_net::http::Request;
use hlin_manifest::envelope::Choice;
use hlin_stream::layout::{CatalogPlatform, ClientConfig, LayoutDocument};

/// What the shell says about itself.
pub async fn config() -> Result<ClientConfig, String> {
    read("/api/config").await
}

/// The surface to open when nobody has said which.
///
/// The shell creates one on a first visit, so this never comes back empty for
/// want of a layout.
pub async fn home() -> Result<LayoutDocument, String> {
    read("/api/layouts/home").await
}

/// One layout by id.
pub async fn layout(id: &str) -> Result<LayoutDocument, String> {
    read(&format!("/api/layouts/{id}")).await
}

/// Every panel that can be put on a surface, grouped by platform.
pub async fn catalog() -> Result<Vec<CatalogPlatform>, String> {
    read("/api/panels").await
}

/// Write a layout, whole.
///
/// The shell answers with what it stored, which is what the caller should
/// believe: panels the browser added come back carrying the identities the
/// shell assigned them.
pub async fn save(document: &LayoutDocument) -> Result<LayoutDocument, String> {
    let id = document
        .id
        .as_deref()
        .ok_or_else(|| "this layout has never been stored".to_string())?;

    let body = serde_json::to_string(document).map_err(|error| error.to_string())?;

    let response = Request::put(&format!("/api/layouts/{id}"))
        .header("content-type", "application/json")
        .body(body)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.ok() {
        return Err(explain(response).await);
    }

    response
        .json()
        .await
        .map_err(|error| format!("the shell's answer could not be read: {error}"))
}

async fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let response = Request::get(path)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.ok() {
        return Err(explain(response).await);
    }

    response
        .json()
        .await
        .map_err(|error| format!("the shell's answer could not be read: {error}"))
}

/// What went wrong, in the shell's own words where it gave any.
///
/// The shell writes a sentence for a person into `error`, and repeating it is
/// better than inventing a second description of the same refusal.
///
/// A refusal for want of a principal is the exception, because there is
/// something to *do* about it rather than something to read. Where the shell
/// signs people in itself it says where, and the browser goes there; where it
/// does not — a proxy was supposed to have said who this is, and did not — no
/// amount of navigating helps and the sentence is all there is. The shell
/// answers the same status in both cases, so the frontend cannot tell them
/// apart without being told, and guessing would send a person round a redirect
/// loop against a shell that has no login at all.
async fn explain(response: gloo_net::http::Response) -> String {
    let status = response.status();
    let body = response.json::<serde_json::Value>().await.ok();

    if status == 401
        && let Some(login) = body
            .as_ref()
            .and_then(|body| body.get("login"))
            .and_then(|where_to| where_to.as_str())
    {
        go_to(login);
        return "signing in…".to_string();
    }

    match body {
        Some(body) => body
            .get("error")
            .and_then(|reason| reason.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("the shell answered {status}")),
        None => format!("the shell answered {status}"),
    }
}

/// Leave, carrying where we were so the shell can put us back.
fn go_to(login: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let here = window
        .location()
        .pathname()
        .unwrap_or_else(|_| "/".to_string());

    let _ = window.location().set_href(&format!("{login}?next={here}"));
}

/// What a control will accept, as the platform listed it.
///
/// One request per control, made once when the catalog arrives rather than per
/// panel: two panels offering the same control ask the shell the same question,
/// and options do not change while somebody is looking at a surface.
///
/// A failure comes back as an empty list rather than an error. The control is
/// still usable — a typed value is what it took before the shell fetched
/// anything — so a platform whose options endpoint is down costs a person the
/// convenience of a menu and nothing else.
pub async fn options(platform: &str, panel: &str, control: &str) -> Vec<Choice> {
    let path = format!("/api/options/{platform}/{panel}/{control}");
    read::<OptionsDocument>(&path)
        .await
        .map(|document| document.choices)
        .unwrap_or_default()
}

/// What the shell answers about a control's values.
#[derive(Debug, serde::Deserialize)]
struct OptionsDocument {
    choices: Vec<Choice>,
}
