//! Everything a widget serves on its origin, as a real platform does
//! ([[HLIN-I-0013]]): its own UI at the root, and Hlin's surface in a subtree.
//!
//! | Path | What | Who is asking |
//! |---|---|---|
//! | `/hlin/…` | Hlin's surface ([`crate::platform`]'s table): manifest, module, `api`, event stream | The shell's `hlin-token`, verified |
//! | `/api/…` | The same `api` handlers and event stream, for the widget's own UI | The platform's own sign-in: in the demo, `--local-user` |
//! | anything else | The widget's own UI: a file of its build, or `index.html` for a route | Anyone |
//!
//! Hlin's subtree is nested before the fallback, and has a 404 fallback of
//! its own, so an unknown path under it (a module file that is not there, a
//! route the widget does not have) is a 404 and never the UI's `index.html`.
//! A shell that asked for a module file and got a web page would try to run
//! it. The UI's fallback answers `index.html` only for a route-like path:
//! `/nope` is the UI, which draws its own not-found, but `/nope.js` is 404,
//! so a stylesheet request never gets HTML.

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use hlin_identity::Claims;

use crate::files::ModuleFiles;
use crate::platform::{LocalUser, Platform, Refusal, health, hlin_routes, own_routes};

/// Where Hlin's surface is, unless configured otherwise.
pub const HLIN_BASE: &str = "/hlin";

/// How a widget's origin is laid out.
#[derive(Debug, Clone)]
pub struct Site {
    /// Where Hlin's surface is: `/hlin`, or another path from the root with
    /// no trailing slash. The shell's `base_url` for the platform ends in it.
    pub hlin_base: String,
    /// Who every request to the widget's own `/api/` is. **Demo only**: the
    /// platform's own sign-in is not what the demo is about, so it stands in
    /// one fixed person. `None`, the default, refuses `/api/` altogether.
    pub local_user: Option<String>,
    /// The widget's own UI, served at the root. `None` for a widget with no
    /// UI of its own yet, which serves a page saying so.
    pub ui: Option<ModuleFiles>,
}

impl Default for Site {
    fn default() -> Self {
        Self {
            hlin_base: HLIN_BASE.to_string(),
            local_user: None,
            ui: None,
        }
    }
}

impl Site {
    /// Why this base cannot be Hlin's, if it cannot: it must be a path from
    /// the root, not the root itself, and not the widget's own `/api`.
    pub fn base_defect(base: &str) -> Option<&'static str> {
        if !base.starts_with('/') || base == "/" || base.ends_with('/') {
            Some("the Hlin base is a path from the root with no trailing slash, like /hlin")
        } else if under(base, "/api") {
            Some("the Hlin base cannot be under /api, which is the widget's own")
        } else {
            None
        }
    }
}

/// The router for a widget's origin, laid out as `site` says (see the module
/// docs).
///
/// # Panics
///
/// If `site.hlin_base` has a [`Site::base_defect`]; [`crate::serve()`] checks
/// before it gets here.
pub fn site<S: Send + 'static>(
    platform: Platform<S>,
    api: Router<Platform<S>>,
    site: Site,
) -> Router {
    if let Some(defect) = Site::base_defect(&site.hlin_base) {
        panic!("{defect}: {:?}", site.hlin_base);
    }
    let hlin = hlin_routes(&platform, api.clone())
        .fallback(not_found)
        .with_state(platform.clone());

    let local = site
        .local_user
        .as_deref()
        .map(|name| local_claims(name, platform.id()));
    let own = own_routes(api)
        .layer(axum::middleware::from_fn(move |request, next| {
            as_local_user(local.clone(), request, next)
        }))
        // After the layer, so health answers whoever asks, as it does under
        // Hlin's base.
        .route("/api/health", get(health))
        .with_state(platform);

    let base = site.hlin_base.clone();
    let ui = site.ui;
    Router::new()
        .nest(&site.hlin_base, hlin)
        .merge(own)
        .fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
            let ui = ui.clone();
            let base = base.clone();
            async move { ui_file(ui.as_ref(), &base, &method, uri.path(), &headers) }
        })
}

/// The origin as [`Site::default`] lays it out: Hlin under `/hlin`, `/api/`
/// refused, and no UI of the widget's own. What a widget's tests start.
pub fn router<S: Send + 'static>(platform: Platform<S>, api: Router<Platform<S>>) -> Router {
    site(platform, api, Site::default())
}

/// Who `--local-user` is, as claims a handler reads like any other.
///
/// Issued by nobody and never verified, which is exactly why it is demo only.
pub fn local_claims(name: &str, audience: &str) -> Claims {
    Claims {
        iss: "local".to_string(),
        sub: format!("local:{name}"),
        aud: audience.to_string(),
        iat: 0,
        exp: i64::MAX,
        jti: "local".to_string(),
        name: Some(name.to_string()),
        email: None,
        groups: Vec::new(),
        extra: Default::default(),
    }
}

/// The widget's own `/api/`: as the local user, or refused.
async fn as_local_user(local: Option<Claims>, mut request: Request, next: Next) -> Response {
    match local {
        Some(claims) => {
            request.extensions_mut().insert(LocalUser(claims));
            next.run(request).await
        }
        None => Refusal::new(
            StatusCode::UNAUTHORIZED,
            "This widget's own sign-in is not part of the demo: start it with --local-user to use its own page",
        )
        .into_response(),
    }
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Whether `path` is `prefix` or below it, by segment: `/hlinx` is not under
/// `/hlin`.
fn under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The widget's own UI's answer for `path`: the file, `index.html` for a
/// route, or 404.
fn ui_file(
    ui: Option<&ModuleFiles>,
    base: &str,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> Response {
    // Never the UI for a path that is Hlin's, the API's or a well-known
    // document's, whether or not something above answered it.
    if [base, "/api", "/.well-known"]
        .iter()
        .any(|prefix| under(path, prefix))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let name = path.trim_start_matches('/');
    let route = !name.rsplit('/').next().unwrap_or(name).contains('.');
    let Some(ui) = ui else {
        return if route {
            placeholder(base)
        } else {
            StatusCode::NOT_FOUND.into_response()
        };
    };
    if !name.is_empty() && ui.has(name) {
        ui.serve(name, headers)
    } else if route {
        ui.serve("index.html", headers)
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// What a widget with no UI of its own yet serves at its root.
fn placeholder(base: &str) -> Response {
    let page = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Widget</title></head><body>\
         <p>This widget has no UI of its own yet. Hlin's surface is under \
         <code>{base}/</code>.</p></body></html>\n"
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        page,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_matched_by_segment_not_by_string() {
        assert!(under("/hlin", "/hlin"));
        assert!(under("/hlin/ui/clock/index.html", "/hlin"));
        assert!(!under("/hlinx", "/hlin"));
        assert!(!under("/apiary", "/api"));
    }

    #[test]
    fn a_base_is_a_path_below_the_root_and_not_the_widgets_own_api() {
        assert_eq!(Site::base_defect("/hlin"), None);
        assert_eq!(Site::base_defect("/integrations/hlin"), None);
        for wrong in ["", "/", "hlin", "/hlin/", "/api", "/api/hlin"] {
            assert!(Site::base_defect(wrong).is_some(), "{wrong:?}");
        }
    }

    #[test]
    fn the_local_user_is_named_as_given_and_marked_as_nobodys_token() {
        let claims = local_claims("Dana", "counter");
        assert_eq!(claims.name.as_deref(), Some("Dana"));
        assert_eq!(claims.sub, "local:Dana");
        assert_eq!(claims.iss, "local");
    }
}
