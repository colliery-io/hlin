//! The values a parameter will accept, fetched on the viewer's behalf.
//!
//! A platform declaring a `select` may name an endpoint listing what it will
//! accept. Until this existed nobody read it, and the control was a text box: a
//! person filtering by cluster had to know that the value was spelled
//! `orebank-eu-west` and type it exactly.
//!
//! The browser cannot fetch it itself. A platform is a different origin, and it
//! expects the shell's credential rather than the viewer's session, so this is
//! the shell doing what it does for panel data — asking as itself, on behalf of
//! whoever is looking (HLIN-A-0004, HLIN-S-0005).
//!
//! # Why the caller names a panel and not a URL
//!
//! The obvious shape for this is `/api/options?url=...`, and it would make the
//! shell an open proxy: anything that can reach this route could ask it to
//! fetch any address the shell can reach, with the shell's own credential
//! attached. So no address arrives from outside. The caller names a platform, a
//! panel and a control, all of which the shell looks up in the manifest it
//! already holds, and the only URL fetched is one that platform declared about
//! itself.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use hlin_manifest::Envelope;
use hlin_manifest::envelope::Choice;

use crate::identity::{Carried, Viewer};
use crate::layouts::Refusal;
use crate::server::AppState;

/// The choices a control offers.
#[derive(Debug, serde::Serialize)]
pub struct OptionsDocument {
    /// What the platform will accept, in the order it listed them.
    pub choices: Vec<Choice>,
}

/// `GET /api/options/{platform_id}/{panel_key}/{control_id}`
///
/// Answers with the platform's list, or says why not. A platform that is down,
/// slow or answering nonsense produces an empty list rather than a failure:
/// the control falls back to accepting a typed value, which is what it did
/// before any of this, so an options endpoint having a bad day costs a person
/// convenience rather than the ability to filter at all.
pub async fn choices(
    State(state): State<AppState>,
    crate::identity::Caller(principal): crate::identity::Caller,
    Path((platform_id, panel_key, control_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<OptionsDocument>, Refusal> {
    let views = state.registry.views().await;
    let Some(view) = views.get(&platform_id) else {
        return Err(Refusal::saying(
            StatusCode::NOT_FOUND,
            format!("no platform `{platform_id}`"),
        ));
    };

    // The panel has to be one the platform currently offers. A layout can name
    // a panel that has been withdrawn, and answering for it would be reading a
    // declaration the platform has retracted.
    let Some(panel) = view.panel(&panel_key) else {
        return Err(Refusal::saying(
            StatusCode::NOT_FOUND,
            format!("`{platform_id}` does not offer `{panel_key}`"),
        ));
    };

    let declared = panel
        .params
        .iter()
        .find(|declaration| {
            declaration.config.get("id").and_then(|id| id.as_str()) == Some(control_id.as_str())
        })
        .and_then(|declaration| declaration.config.get("options"))
        .and_then(|options| options.as_str());

    let Some(path) = declared else {
        // Not an error: most controls list nothing, and a caller asking about
        // one is asking a reasonable question with a boring answer.
        return Ok(Json(OptionsDocument { choices: vec![] }));
    };

    let credential = view.credentialer.headers(&Viewer {
        principal,
        carried: Carried::from_cookie_header(
            headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok()),
        ),
    });

    let credential = match credential {
        Ok(headers) => headers,
        Err(reason) => {
            tracing::warn!(platform = platform_id, %reason, "no credential for an options fetch");
            return Ok(Json(OptionsDocument { choices: vec![] }));
        }
    };

    let url = format!(
        "{}/{}",
        view.config.base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    let mut request = state.client.get(&url);
    for (name, value) in credential {
        request = request.header(name, value);
    }

    let answered = match request.send().await {
        Ok(answer) if answer.status().is_success() => {
            // Bounded for the same reason the panel path is: this is the shell
            // calling a platform with its own credential, and an options
            // endpoint is no more trustworthy about length than a data one.
            match crate::bounded::read_bounded(answer, hlin_manifest::envelope::MAX_DOCUMENT_BYTES)
                .await
            {
                Ok(body) => Some(body),
                Err(reason) => {
                    tracing::debug!(platform = platform_id, %reason, "options body refused");
                    None
                }
            }
        }
        Ok(answer) => {
            tracing::debug!(platform = platform_id, status = %answer.status(), "options refused");
            None
        }
        Err(error) => {
            tracing::debug!(platform = platform_id, %error, "options unreachable");
            None
        }
    };

    let choices = answered
        .and_then(|body| {
            hlin_manifest::parse_envelope(&body, hlin_manifest::envelope::OPTIONS_V1).ok()
        })
        .and_then(|envelope| match envelope {
            Envelope::Options(listed) => Some(listed.options),
            // A platform answering something other than `options.v1` here has
            // a bug, and drawing its scalar as a list of choices would hide it.
            other => {
                tracing::debug!(
                    platform = platform_id,
                    envelope = other.name(),
                    "an options endpoint answered something else"
                );
                None
            }
        })
        .unwrap_or_default();

    Ok(Json(OptionsDocument { choices }))
}
