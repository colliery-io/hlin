//! What the shell attaches when it calls a platform.
//!
//! Authentication is hoisted to Hlin and platforms authorize at fetch time
//! (decision HLIN-A-0004). *How* the shell says who is asking is a per-platform
//! strategy (decision HLIN-A-0008), so a platform already behind the shared
//! session needs no change on day one and adopts the token on its own
//! schedule.
//!
//! Everything downstream of here sees a `Principal` and a `HeaderMap`. The
//! aggregator never learns which strategy produced them, which is what keeps
//! adding a strategy to one file.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::response::IntoResponse;
use hlin_identity::{IDENTITY_HEADER, Issuer, Principal};
use serde::{Deserialize, Serialize};

/// How the shell identifies itself to one platform.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "strategy", rename_all = "kebab-case")]
pub enum CredentialConfig {
    /// Forward the viewer's own session cookies.
    ///
    /// The day-one default where a platform shares the shell's session,
    /// because it needs no change on the platform at all.
    ForwardSession {
        /// Which cookies to forward. Only these; never the whole jar.
        cookies: Vec<String>,
    },

    /// Mint a signed token per request (HLIN-S-0004).
    ///
    /// The long-term default, and the only strategy usable across a trust
    /// boundary.
    #[default]
    HlinToken,

    /// Nothing at all.
    ///
    /// For a platform that is open: public data, no account, nothing to
    /// present. The other three strategies all say *how* the shell proves who
    /// is asking, and against a platform that does not ask, every one of them
    /// is an answer to a question nobody put. `hlin-token` would work — a
    /// platform ignores a header it does not read — but it signs a token per
    /// request for nobody, and it makes the configuration claim a relationship
    /// that does not exist.
    ///
    /// Nothing to acknowledge, unlike `static-bearer`: this collapses no
    /// principals because it carries none. Every viewer reaches the platform
    /// as nobody, which is what the platform already offers everybody.
    None,

    /// A fixed key, the same for every viewer.
    StaticBearer {
        /// The environment variable holding the key. Never the key itself, so
        /// a configuration file is not a secret.
        token_env: String,
        /// Required. One principal for everyone means the platform's own
        /// per-user rules cannot apply, `forbidden` can never occur, and
        /// deduplication is global for this platform. That is a decision, not
        /// a default.
        #[serde(default)]
        acknowledge_shared_principal: bool,
    },
}

impl CredentialConfig {
    /// The strategy's name, for logs and for `/api/platforms`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::ForwardSession { .. } => "forward-session",
            Self::HlinToken => "hlin-token",
            Self::None => "none",
            Self::StaticBearer { .. } => "static-bearer",
        }
    }

    /// Whether this strategy makes every viewer the same caller.
    ///
    /// Read by the aggregator: where it is true, deduplication is global for
    /// that platform, because the platform genuinely cannot tell viewers apart.
    pub fn collapses_principals(&self) -> bool {
        matches!(self, Self::StaticBearer { .. })
    }

    /// Check what can be checked before any request is made.
    pub fn check(&self, platform: &str, base_url: &str, shell_port: u16) -> Result<(), String> {
        match self {
            Self::ForwardSession { cookies } => {
                if cookies.is_empty() {
                    return Err("forward-session names no cookies to forward".to_string());
                }

                // A cookie is a bearer credential for everything on its origin.
                // Sending one anywhere else hands that everything to whoever
                // answers, so this is refused rather than warned about.
                if !is_same_origin_as_shell(base_url, shell_port) {
                    return Err(format!(
                        "forward-session requires the platform to be same-origin with the shell, \
                         and `{base_url}` is not; use hlin-token across an origin boundary"
                    ));
                }
                Ok(())
            }

            Self::HlinToken => Ok(()),

            // Nothing to check. There is no secret to find in the environment,
            // no origin to compare, and no consequence to acknowledge.
            Self::None => Ok(()),

            Self::StaticBearer {
                token_env,
                acknowledge_shared_principal,
            } => {
                if !acknowledge_shared_principal {
                    return Err(
                        "static-bearer makes every viewer the same caller, so the platform's own \
                         per-user rules cannot apply and no viewer can ever be told they lack \
                         access; set acknowledge_shared_principal = true to accept that"
                            .to_string(),
                    );
                }
                if std::env::var(token_env).is_err() {
                    return Err(format!(
                        "static-bearer expects `{token_env}` in the environment"
                    ));
                }
                let _ = platform;
                Ok(())
            }
        }
    }
}

/// Whether a platform is on the shell's own origin.
///
/// Deliberately strict: the demo runs everything on `127.0.0.1` with different
/// ports, which is *not* same-origin, so the demo's session platform is
/// configured with the shell's own port or the check refuses it. A production
/// shell sits in front of path-prefixed platforms on one host, where this is
/// naturally true.
fn is_same_origin_as_shell(base_url: &str, shell_port: u16) -> bool {
    let Some(rest) = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
    else {
        return false;
    };

    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok()),
        None => (authority, None),
    };

    let same_host = matches!(host, "localhost" | "127.0.0.1" | "::1")
        || host == std::env::var("HLIN_SHELL_HOST").unwrap_or_default();

    same_host && port.map(|port| port == shell_port).unwrap_or(true)
}

/// What a viewer brought with them, held only for the life of their stream.
///
/// Never logged, never persisted. The forwarding strategies need it; the token
/// strategy does not.
#[derive(Debug, Clone, Default)]
pub struct Carried {
    /// The cookies the browser sent, by name.
    pub cookies: BTreeMap<String, String>,
}

impl Carried {
    /// Read the cookies off a request's `Cookie` header.
    pub fn from_cookie_header(header: Option<&str>) -> Self {
        let cookies = header
            .unwrap_or("")
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        Self { cookies }
    }
}

/// Everything the shell knows about who is asking.
#[derive(Debug, Clone)]
pub struct Viewer {
    /// Who they are.
    pub principal: Principal,
    /// What they brought, for the forwarding strategies.
    pub carried: Carried,
}

impl Viewer {
    /// A viewer with nothing carried, for tests and for the token strategy.
    pub fn new(principal: Principal) -> Self {
        Self {
            principal,
            carried: Carried::default(),
        }
    }
}

/// Turns a viewer into the headers a request to one platform carries.
pub trait Credentialer: Send + Sync {
    /// The headers, or why they could not be produced.
    fn headers(&self, viewer: &Viewer) -> Result<Vec<(String, String)>, String>;

    /// The strategy's name, for logs.
    fn name(&self) -> &'static str;

    /// Whether every viewer reaches the platform as the same caller.
    fn collapses_principals(&self) -> bool {
        false
    }
}

/// Build the credentialer a platform's configuration asks for.
pub fn build(
    config: &CredentialConfig,
    platform_id: &str,
    issuer: Arc<Issuer>,
) -> Result<Box<dyn Credentialer>, String> {
    Ok(match config {
        CredentialConfig::ForwardSession { cookies } => Box::new(ForwardSession {
            cookies: cookies.clone(),
        }),
        CredentialConfig::HlinToken => Box::new(HlinToken {
            issuer,
            audience: platform_id.to_string(),
        }),
        CredentialConfig::None => Box::new(Nothing),
        CredentialConfig::StaticBearer { token_env, .. } => {
            let token = std::env::var(token_env)
                .map_err(|_| format!("`{token_env}` is not in the environment"))?;
            Box::new(StaticBearer { token })
        }
    })
}

/// Forwards the viewer's session cookies.
struct ForwardSession {
    cookies: Vec<String>,
}

impl Credentialer for ForwardSession {
    fn headers(&self, viewer: &Viewer) -> Result<Vec<(String, String)>, String> {
        let jar: Vec<String> = self
            .cookies
            .iter()
            .filter_map(|name| {
                viewer
                    .carried
                    .cookies
                    .get(name)
                    .map(|value| format!("{name}={value}"))
            })
            .collect();

        if jar.is_empty() {
            // The viewer has no session for this platform. Sending an empty
            // cookie header would look like a request from nobody, which the
            // platform would refuse anyway; saying so here gives an operator
            // the reason.
            return Err("the viewer carries none of the configured cookies".to_string());
        }

        Ok(vec![("cookie".to_string(), jar.join("; "))])
    }

    fn name(&self) -> &'static str {
        "forward-session"
    }
}

/// Mints a token per request.
struct HlinToken {
    issuer: Arc<Issuer>,
    audience: String,
}

impl Credentialer for HlinToken {
    fn headers(&self, viewer: &Viewer) -> Result<Vec<(String, String)>, String> {
        // Per request, never per stream: a browser stream outlives any sane
        // token lifetime (HLIN-S-0004 REQ-1.3).
        let token = self
            .issuer
            .mint(&viewer.principal, &self.audience)
            .map_err(|error| error.to_string())?;

        Ok(vec![(IDENTITY_HEADER.to_lowercase(), token)])
    }

    fn name(&self) -> &'static str {
        "hlin-token"
    }
}

/// Nothing at all, for a platform that asks for nothing.
struct Nothing;

impl Credentialer for Nothing {
    fn headers(&self, _viewer: &Viewer) -> Result<Vec<(String, String)>, String> {
        Ok(Vec::new())
    }

    fn name(&self) -> &'static str {
        "none"
    }
}

/// One key for everyone.
struct StaticBearer {
    token: String,
}

impl Credentialer for StaticBearer {
    fn headers(&self, _viewer: &Viewer) -> Result<Vec<(String, String)>, String> {
        Ok(vec![(
            "authorization".to_string(),
            format!("Bearer {}", self.token),
        )])
    }

    fn name(&self) -> &'static str {
        "static-bearer"
    }

    fn collapses_principals(&self) -> bool {
        true
    }
}

// -- Who is asking ---------------------------------------------------------

/// The principal a request is made on behalf of.
///
/// An extractor rather than something a handler fetches, and that is the point.
/// Every handler used to call a no-argument `Config::principal()` and get the
/// same person for every request — so ownership, visibility and per-viewer
/// deduplication were implemented, tested, and unable to do anything at all.
///
/// A handler that needs to know who is asking now says so in its signature, and
/// a request that carries no principal is refused with 401 before the handler
/// runs. There is no way to write a handler that forgets, because there is no
/// longer anything to forget: the principal only exists as an argument.
pub struct Caller(pub hlin_identity::Principal);

impl axum::extract::FromRequestParts<crate::server::AppState> for Caller {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        app: &crate::server::AppState,
    ) -> Result<Self, Self::Rejection> {
        match app.config.principal_from(&parts.headers) {
            crate::config::Asking::Known(principal) => Ok(Caller(principal)),

            // A session the shell issued. The cookie is a name for an identity
            // the shell wrote down, so this is where it reads it back.
            crate::config::Asking::Session(value) => {
                match app.store.session(&crate::auth::fingerprint(&value)).await {
                    Ok(Some(session)) => {
                        let mut principal = hlin_identity::Principal::new(session.subject);
                        principal.name = session.name;
                        principal.groups = session.groups;
                        Ok(Caller(principal))
                    }
                    // A cookie naming no session is somebody whose session
                    // ended — expired, swept, or signed out — and is refused
                    // exactly like one who never had one. It is not an error
                    // and says nothing worth logging.
                    Ok(None) => Err(refuse(app)),
                    Err(error) => {
                        // The store being down must not be reported as "you are
                        // not signed in", which would send everyone to sign in
                        // again and produce a stampede at the provider on top
                        // of whatever is already wrong.
                        tracing::error!(%error, "could not read a session");
                        Err(axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response())
                    }
                }
            }

            crate::config::Asking::Nobody => Err(refuse(app)),
        }
    }
}

/// Refuse a request that carries nobody, saying where a person could go.
///
/// The status is the answer; the body is a courtesy to the frontend, which
/// otherwise has no way to tell "sign in over there" from "the proxy in front
/// of this shell is broken" — two situations that want opposite responses and
/// look identical from a bare 401.
fn refuse(app: &crate::server::AppState) -> axum::response::Response {
    let login = matches!(app.config.auth, crate::config::AuthConfig::Oidc(_))
        .then_some(crate::config::LOGIN_PATH);

    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "login": login })),
    )
        .into_response()
}

/// The principal, plus the right to change something.
///
/// A separate extractor from [`Caller`] rather than a check inside each
/// handler, and for the reason [`Caller`] itself exists: a handler that mutates
/// says so in its signature, and the guard cannot be forgotten because there is
/// nothing to remember — the only way to get a principal in a mutating handler
/// is to ask for one that came with the right to mutate.
///
/// A shell using `anonymous` refuses every write (HLIN-A-0012). Nobody signed
/// in, so nothing can be owned; a visitor able to create, edit and delete would
/// be able to delete the surfaces every other visitor came to see. The
/// alternative — anonymous callers writing freely — is what `dev` does, and
/// what a release build refuses `dev` for.
///
/// This is authentication, not authorisation: whether *this* person may edit
/// *that* layout is still the layout's own question, asked further in.
pub struct Author(pub hlin_identity::Principal);

impl axum::extract::FromRequestParts<crate::server::AppState> for Author {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        app: &crate::server::AppState,
    ) -> Result<Self, Self::Rejection> {
        // Who, first. A read-only shell still answers "you are not signed in"
        // ahead of "and you could not write anyway", because the first is the
        // more useful thing to be told and the second is true of everyone.
        let Caller(principal) = Caller::from_request_parts(parts, app).await?;

        if app.config.read_only() {
            return Err((
                axum::http::StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({
                    "error": "read_only",
                    "detail": "This shell is open to anybody and keeps nothing: nobody signs \
                               in, so nothing can be owned, and a visitor able to edit could \
                               delete the surfaces everyone else came to see. Surfaces are \
                               composed against the same database by a shell configured with \
                               an authenticator.",
                })),
            )
                .into_response());
        }

        Ok(Author(principal))
    }
}
