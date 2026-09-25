//! Relaying a module's `changed` to every surface this shell serves
//! (specification HLIN-S-0007, *Module to shell*, `changed`).
//!
//! A module that wrote something says so, and the other modules of its
//! platform should refetch — including the ones in other people's browsers,
//! which the page that heard it cannot reach. So the page posts it here, and
//! the shell puts it beside the platforms' own events
//! ([`crate::stream::streams::Streams::relay`]), where every running surface
//! hears it on its next tick and passes it to its browser as a `changed` frame
//! (HLIN-S-0003).
//!
//! The checks are the request proxy's, for the same reasons: only the shell's
//! own page may post (a module has no network, and nothing else should be able
//! to make a shell tell every browser to refetch), only someone who could have
//! written, and only for a platform whose module is on a layout that person can
//! see. The module never names its platform; the page does, from which frame
//! the message came.

use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hlin_manifest::Access;

use crate::identity::Author;
use crate::modules::requests::{Refusal, check_caller};
use crate::server::AppState;
use crate::stream::events::Changed;
use crate::stream::streams::Relayed;

/// The most a relayed change may be, as JSON.
///
/// A panel key and a few selections. Far more than any honest one needs, and
/// small enough that nothing a page sends here costs the shell anything to
/// hold while every surface reads it.
pub const MOST_PER_CHANGE: usize = 16 * 1024;

/// `POST /api/stream/{surface_id}/changed`
///
/// Answers `202` once the change is on its way; delivery is best-effort, like
/// a platform's own event, because a surface that misses one is merely one
/// refetch behind.
pub async fn relay(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
    request: Request,
) -> Response {
    match relayed(&state, &surface_id, request).await {
        Ok(listening) => {
            tracing::debug!(surface = surface_id, listening, "relayed a module's change");
            StatusCode::ACCEPTED.into_response()
        }
        Err(refused) => refused,
    }
}

async fn relayed(state: &AppState, surface_id: &str, request: Request) -> Result<usize, Response> {
    let (mut parts, body) = request.into_parts();

    // Only the shell's own page, held to the rules for a write: a change is
    // news of one.
    check_caller(state, &parts.headers, Some(Access::Write))
        .map_err(IntoResponse::into_response)?;

    // Someone who could have written. A read-only shell refuses every write, so
    // nobody on it has anything to report.
    let Author(principal) = match Author::from_request_parts(&mut parts, state).await {
        Ok(author) => author,
        Err(refused) if refused.status() == StatusCode::UNAUTHORIZED => {
            return Err(Refusal::not_signed_in().into_response());
        }
        Err(refused) if refused.status() == StatusCode::FORBIDDEN => {
            return Err(Refusal::new(
                "read_only",
                StatusCode::FORBIDDEN,
                "this shell takes no writes, so nothing changed",
            )
            .into_response());
        }
        Err(other) => return Err(other),
    };

    let bytes = axum::body::to_bytes(body, MOST_PER_CHANGE)
        .await
        .map_err(|_| {
            Refusal::new(
                "too_large",
                StatusCode::PAYLOAD_TOO_LARGE,
                "a change is a panel and its selections, and this is more",
            )
            .into_response()
        })?;
    let change: hlin_stream::ChangedRequest = serde_json::from_slice(&bytes).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": format!("not a change: {error}") })),
        )
            .into_response()
    })?;
    if change.panel.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "a change names the panel it concerns" })),
        )
            .into_response());
    }

    // A platform whose module is on this layout, as this person sees it.
    // Anything else is refused the same way, so the answer says nothing about
    // layouts the caller cannot see.
    let holds = state
        .surfaces
        .holds_module(surface_id, &principal, state, &change.platform)
        .await
        .unwrap_or(false);
    if !holds {
        return Err((
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("no module of `{}` on this surface", change.platform),
            })),
        )
            .into_response());
    }

    Ok(state.streams.relay(Relayed {
        platform_id: change.platform,
        changed: Changed {
            panel: change.panel,
            selections: change.selections,
        },
        page: change.page,
    }))
}
