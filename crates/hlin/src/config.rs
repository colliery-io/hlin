//! What the shell is told at startup.
//!
//! One file, read once. Platform discovery is configuration rather than a
//! registry protocol because the vision put it behind a trait and said so: a
//! push model or orchestrator labels can replace this later without reshaping
//! anything above it.
//!
//! Rules that would otherwise fail at the first request are checked here, when
//! an operator is watching, and refused with the platform named.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::identity::CredentialConfig;

/// Everything the shell needs to run.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Which address the shell listens on.
    ///
    /// Loopback by default, because a shell started by a developer should not
    /// be on the network by accident. A deployment sets this — a container in
    /// particular, where loopback means the container's own and nothing outside
    /// it can reach the shell at all. That was the first thing running the
    /// image found, after it built and started and looked entirely healthy.
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Where the shell listens.
    #[serde(default = "default_port")]
    pub port: u16,

    /// What the shell calls itself, in the `iss` of every token it mints.
    #[serde(default = "default_issuer")]
    pub issuer: String,

    /// Where the shell's signing key lives, so a restart keeps its `kid`.
    #[serde(default = "default_key_path")]
    pub key_path: PathBuf,

    /// Postgres. Absent means run without persistence, which is only sensible
    /// in development and is warned about at startup.
    ///
    /// A connection string carries a password, so anywhere it is deployed from
    /// a file that file is a secret. Prefer [`Config::database_url_env`].
    #[serde(default)]
    pub database_url: Option<String>,

    /// The environment variable holding the connection string.
    ///
    /// The same shape `client_secret_env` and `token_env` already use, for the
    /// same reason: a configuration file that has to be treated as a secret is
    /// one somebody eventually forgets to treat as a secret. Set, this wins over
    /// the literal above; a named variable that is not in the environment is
    /// refused at startup rather than quietly leaving the shell with no
    /// database — which looks identical to not having configured one.
    #[serde(default)]
    pub database_url_env: Option<String>,

    /// Where the built frontend is, which is also which frontend to serve.
    ///
    /// A front end is a binary somebody built by choosing a design pack and
    /// mounting `hlin-ui`. The shell serves whichever one it is pointed at and
    /// has no opinion about which, so swapping the design system is a
    /// configuration change here rather than a change to the shell.
    #[serde(default = "default_frontend")]
    pub frontend: PathBuf,

    /// How a person becomes a principal.
    #[serde(default)]
    pub auth: AuthConfig,

    /// Timings, all with defaults from the specifications.
    #[serde(default)]
    pub timings: Timings,

    /// The platforms this shell knows about.
    #[serde(default)]
    pub platforms: Vec<PlatformConfig>,
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_issuer() -> String {
    "hlin".to_string()
}

fn default_frontend() -> PathBuf {
    PathBuf::from("examples/frontend-demo/dist")
}

fn default_key_path() -> PathBuf {
    PathBuf::from("demo/state/shell.key")
}

/// How a person becomes a principal.
///
/// `oidc` is specified in HLIN-S-0005 and is not here yet; see the task that
/// added `trusted-header` for why it was separated rather than rushed.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "strategy", rename_all = "kebab-case")]
pub enum AuthConfig {
    /// A fixed principal from configuration.
    ///
    /// Every request is the same person. That is exactly what a developer
    /// wants and exactly what nobody should ever deploy, so
    /// [`AuthConfig::check`] refuses it outside a debug build — the same rule
    /// `hlin-identity` applies to its own development bypass, for the same
    /// reason: a flag that can be set at runtime is a flag that will be set, at
    /// three in the morning, to make something work.
    Dev {
        /// Who everyone is.
        #[serde(default = "default_dev_sub")]
        sub: String,
        /// Their display name.
        #[serde(default)]
        name: Option<String>,
        /// Their groups, which platforms may use for their own decisions.
        #[serde(default)]
        groups: Vec<String>,
    },

    /// The principal is in a header a proxy guarantees.
    ///
    /// The smallest real strategy, and the one that unblocks a deployment that
    /// already has an authenticating proxy or a mesh in front of it. Its whole
    /// security rests on nothing being able to reach the shell except through
    /// that proxy — a header is trivially forged by anything that can connect
    /// directly — which is why the configuration has to say so out loud
    /// (HLIN-S-0005).
    TrustedHeader {
        /// The header carrying the principal's identifier.
        #[serde(default = "default_principal_header")]
        header: String,

        /// A header carrying their groups, comma separated, where the proxy
        /// supplies them.
        #[serde(default)]
        groups_header: Option<String>,

        /// A header carrying their display name.
        #[serde(default)]
        name_header: Option<String>,

        /// That the deployment guarantees the shell is unreachable except
        /// through the proxy that sets these headers.
        ///
        /// Required, and not merely documented, because the failure is silent:
        /// a shell reachable directly accepts whoever a caller claims to be,
        /// and nothing about that looks wrong from the inside.
        #[serde(default)]
        acknowledge_proxy_required: bool,
    },

    /// The shell authenticates people itself, against an OpenID Connect
    /// provider, and issues its own session (HLIN-S-0005).
    ///
    /// The strategy for a shell that is not already behind something that knows
    /// who everybody is — and the only one that can hold the provider's tokens,
    /// which the forwarding credentialers need.
    Oidc(OidcConfig),
}

/// Everything the `oidc` authenticator needs.
///
/// Its own struct rather than a wide variant, because half of it is a promise
/// to the deployment rather than a knob: where the shell is, how long a session
/// lasts, and how its cookie is scoped.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OidcConfig {
    /// The provider, as it names itself.
    ///
    /// Discovery is read from `{issuer}/.well-known/openid-configuration`, and
    /// the `iss` claim of every id token is checked against it.
    pub issuer: String,

    /// The client this shell is registered as.
    pub client_id: String,

    /// The environment variable holding the client secret.
    ///
    /// Never the secret itself, so a configuration file is not a secret — the
    /// rule `static-bearer` already follows, for the same reason.
    pub client_secret_env: String,

    /// Where this shell is, as a browser reaches it.
    ///
    /// Required, and not derivable. The redirect URI has to match what was
    /// registered with the provider exactly, and a shell behind a proxy cannot
    /// see its own public address from the inside.
    pub public_url: String,

    /// What to ask the provider for.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// The claim carrying group membership, which platforms may use for their
    /// own decisions.
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,

    /// How long a session lasts before the person signs in again.
    #[serde(default = "default_session_hours")]
    pub session_hours: u64,

    /// How the session cookie is scoped.
    #[serde(default)]
    pub cookie: CookieConfig,
}

impl OidcConfig {
    /// Where the provider returns the browser to.
    ///
    /// Built rather than configured, so it cannot drift from the route that
    /// actually exists — but built from `public_url`, which must be configured,
    /// because the shell cannot see its own public address from the inside.
    pub fn redirect_uri(&self) -> String {
        format!("{}{}", self.public_url.trim_end_matches('/'), CALLBACK_PATH)
    }

    /// How long a session lasts.
    pub fn session_life(&self) -> chrono::Duration {
        chrono::Duration::hours(self.session_hours as i64)
    }
}

/// Where the provider sends a browser back to.
pub const CALLBACK_PATH: &str = "/auth/callback";

/// Where a browser that is nobody is sent to become somebody.
pub const LOGIN_PATH: &str = "/auth/login";

/// How the shell's own session cookie is set.
///
/// Each of these is a promise to the deployment rather than a preference, which
/// is why they are named and defaulted here rather than written inline at the
/// point the header is built.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CookieConfig {
    /// What it is called.
    pub name: String,

    /// Whether the browser may send it over plain HTTP.
    ///
    /// Defaults to refusing, and [`AuthConfig::check`] refuses a shell served
    /// over `https` whose cookie is not `Secure`. The cookie is the whole of a
    /// session; sending it in the clear hands the session to anything on the
    /// path between the person and the shell.
    pub secure: bool,

    /// The `SameSite` attribute.
    ///
    /// `Lax`, and the sign-in flow is the reason it is not `Strict`: the
    /// provider returns the browser by a top-level redirect, which `Strict`
    /// treats as cross-site and strips the cookie from — so a person arrives
    /// back at the shell signed in and apparently not, forever. `Lax` still
    /// withholds it from cross-site sub-requests, which is the attack the
    /// attribute exists for.
    pub same_site: String,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            name: "hlin_session".to_string(),
            secure: true,
            same_site: "Lax".to_string(),
        }
    }
}

fn default_scopes() -> Vec<String> {
    ["openid", "profile", "email", "groups"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn default_groups_claim() -> String {
    "groups".to_string()
}

fn default_session_hours() -> u64 {
    12
}

fn default_principal_header() -> String {
    "x-forwarded-user".to_string()
}

impl AuthConfig {
    /// What this strategy is called, for logs and for `hlin check`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Dev { .. } => "dev",
            Self::TrustedHeader { .. } => "trusted-header",
            Self::Oidc(_) => "oidc",
        }
    }

    /// Whether this strategy may be used, given how the shell was built.
    ///
    /// Checked at startup, where an operator is watching, rather than at the
    /// first request.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Dev { .. } if !cfg!(debug_assertions) => Err(
                "the `dev` authenticator makes every request the same person and must never \
                 be used outside development; this is a release build. Use `trusted-header` \
                 behind a proxy that authenticates."
                    .to_string(),
            ),
            Self::Dev { .. } => Ok(()),

            Self::TrustedHeader {
                header,
                acknowledge_proxy_required,
                ..
            } => {
                if header.trim().is_empty() {
                    return Err("trusted-header names no header to read".to_string());
                }
                if !acknowledge_proxy_required {
                    return Err(
                        "trusted-header trusts whatever the request says it is, so a shell \
                         reachable without going through the proxy accepts anyone as anyone; \
                         set acknowledge_proxy_required = true to accept that"
                            .to_string(),
                    );
                }
                Ok(())
            }

            Self::Oidc(oidc) => oidc.check(),
        }
    }
}

impl OidcConfig {
    /// What can be checked before anybody tries to sign in.
    ///
    /// Everything here fails at the first sign-in otherwise, which is both
    /// later and quieter: a shell that starts and then refuses everyone looks
    /// like an outage, and the operator who could have fixed it in a second has
    /// long stopped watching.
    fn check(&self) -> Result<(), String> {
        if !self.issuer.starts_with("https://") {
            // The issuer is where the shell fetches the keys it will trust to
            // say who someone is. Over plain HTTP, anything on the path chooses
            // those keys.
            return Err(format!(
                "the oidc issuer must be https, and `{}` is not",
                self.issuer
            ));
        }

        if self.client_id.trim().is_empty() {
            return Err("oidc names no client_id".to_string());
        }

        if std::env::var(&self.client_secret_env).is_err() {
            return Err(format!(
                "oidc expects the client secret in `{}`, which is not in the environment",
                self.client_secret_env
            ));
        }

        if !self.public_url.starts_with("http://") && !self.public_url.starts_with("https://") {
            return Err(format!(
                "oidc needs the shell's own public address to build a redirect URI, \
                 and `{}` is not one",
                self.public_url
            ));
        }

        if self.public_url.starts_with("https://") && !self.cookie.secure {
            return Err(
                "this shell is served over https and its session cookie is not Secure, so a \
                 browser will send the whole of a session over plain HTTP to anything that can \
                 get it to try; set auth.cookie.secure = true"
                    .to_string(),
            );
        }

        if !self.scopes.iter().any(|scope| scope == "openid") {
            return Err(
                "oidc scopes must include `openid`, or the provider returns no id token and \
                 there is nothing to learn who anybody is from"
                    .to_string(),
            );
        }

        if self.session_hours == 0 {
            return Err(
                "a session that lasts no time at all signs everyone out at once; \
                        auth.session_hours must be at least 1"
                    .to_string(),
            );
        }

        match self.cookie.same_site.as_str() {
            "Lax" | "None" => Ok(()),
            "Strict" => Err(
                "SameSite=Strict strips the session cookie from the redirect the provider uses \
                 to return the browser, so a person would arrive back at the shell signed in and \
                 apparently not, forever; use Lax"
                    .to_string(),
            ),
            other => Err(format!("`{other}` is not a SameSite value")),
        }
    }
}

fn default_dev_sub() -> String {
    "u_dev".to_string()
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::Dev {
            sub: default_dev_sub(),
            name: Some("Development User".to_string()),
            groups: vec!["platform-engineering".to_string()],
        }
    }
}

/// One platform the shell polls.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlatformConfig {
    /// The identity configuration assigns to this platform.
    ///
    /// Authoritative: a manifest claiming a different id is malformed
    /// (HLIN-S-0001 REQ-1.5).
    pub id: String,

    /// Where it lives.
    pub base_url: String,

    /// What credential the shell attaches when calling it (HLIN-S-0005).
    #[serde(default)]
    pub auth: CredentialConfig,
}

/// The intervals from the specifications, all overridable.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Timings {
    /// How often each manifest is refetched.
    #[serde(default = "default_poll_seconds")]
    pub poll_seconds: u64,

    /// How many consecutive polls must agree before a contract change is
    /// classified, so a blue/green rollout does not raise a violation per flip
    /// (decision HLIN-A-0002).
    #[serde(default = "default_debounce")]
    pub debounce_observations: i32,

    /// How long the shell waits on a platform before calling it unreachable.
    #[serde(default = "default_timeout_seconds")]
    pub upstream_timeout_seconds: u64,

    /// How often a panel on a watched surface is refetched.
    ///
    /// In milliseconds because a live panel is a sub-second thing and the rest
    /// of this struct is not. The driver in `stream::live` looks for work on a
    /// tick derived from this, so a value below about ten milliseconds buys
    /// nothing but load.
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,

    /// The fastest any panel may be refetched, however often its platform asks
    /// (decision HLIN-A-0009).
    ///
    /// An absolute bound rather than a fraction of `refresh_ms`, and that
    /// distinction is the whole point of the field. Deriving it from the
    /// shell's own interval means a shell polling every thirty seconds clamps a
    /// panel that asked for 125ms to seven and a half — so the slower the
    /// shell's default, the less a fast panel is allowed to say, which is
    /// exactly backwards. Measured on the demo before this was fixed: two
    /// frames in ten seconds where eighty were expected.
    ///
    /// What this protects is the shell's own load, and that does not get more
    /// or less urgent because the default interval changed.
    #[serde(default = "default_refresh_floor_ms")]
    pub refresh_floor_ms: u64,

    /// How long data may go unrefreshed before a panel says so on its face.
    ///
    /// Worth keeping proportional to `refresh_ms` rather than absolute: at
    /// eight refreshes a second, the ninety seconds that suits a half-minute
    /// poll is seven hundred missed frames spent looking healthy.
    #[serde(default = "default_staleness_ms")]
    pub staleness_ms: u64,

    /// The first wait after a platform fails, and the longest one backoff
    /// grows to. A failing platform is asked on this schedule rather than on
    /// `refresh_ms`, so a fast surface does not become a fast retry storm.
    #[serde(default = "default_retry_from_ms")]
    pub retry_from_ms: u64,

    /// The ceiling backoff grows to.
    #[serde(default = "default_retry_ceiling_ms")]
    pub retry_ceiling_ms: u64,
}

fn default_poll_seconds() -> u64 {
    30
}

fn default_debounce() -> i32 {
    2
}

fn default_timeout_seconds() -> u64 {
    10
}

fn default_refresh_ms() -> u64 {
    30_000
}

fn default_refresh_floor_ms() -> u64 {
    100
}

fn default_staleness_ms() -> u64 {
    90_000
}

fn default_retry_from_ms() -> u64 {
    30_000
}

fn default_retry_ceiling_ms() -> u64 {
    300_000
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            poll_seconds: default_poll_seconds(),
            debounce_observations: default_debounce(),
            upstream_timeout_seconds: default_timeout_seconds(),
            refresh_ms: default_refresh_ms(),
            refresh_floor_ms: default_refresh_floor_ms(),
            staleness_ms: default_staleness_ms(),
            retry_from_ms: default_retry_from_ms(),
            retry_ceiling_ms: default_retry_ceiling_ms(),
        }
    }
}

impl Timings {
    /// The poll interval.
    pub fn poll(&self) -> Duration {
        Duration::from_secs(self.poll_seconds)
    }

    /// The upstream timeout.
    pub fn upstream_timeout(&self) -> Duration {
        Duration::from_secs(self.upstream_timeout_seconds)
    }

    /// What a watched surface should do about time.
    ///
    /// This exists because the refresh interval used to be a constant in
    /// `stream::aggregator`, reachable from nowhere, while this struct claimed
    /// every interval was overridable. A shell serving a panel that changes
    /// eight times a second and one that changes nightly should not be made to
    /// treat them the same, and until a platform can say which it is
    /// (HLIN-A-0002 territory: the manifest has no cadence field), the operator
    /// saying so per shell is the honest interim.
    pub fn policy(&self) -> crate::stream::aggregator::Policy {
        use chrono::Duration as Signed;

        let millis = |value: u64| Signed::milliseconds(value.min(i64::MAX as u64) as i64);
        let default = crate::stream::aggregator::Policy::default();

        crate::stream::aggregator::Policy {
            refresh: millis(self.refresh_ms),
            // The floor a panel's own cadence is clamped to, and the floor an
            // event-prompted fetch is held to as well. Absolute rather than a
            // fraction of the shell's interval: deriving it made a fast panel
            // *slower* the slower the shell's default was, which is exactly
            // backwards (decision HLIN-A-0009).
            refresh_floor: millis(self.refresh_floor_ms),
            staleness: millis(self.staleness_ms),
            retry_from: millis(self.retry_from_ms),
            retry_ceiling: millis(self.retry_ceiling_ms),
            // Neither of these is exposed, for the same reason. Settling is
            // about how fast a hand moves a picker, and coalescing is about not
            // turning one notification into a burst — both properties of people
            // and arithmetic rather than of a deployment.
            settle: default.settle,
            coalesce: default.coalesce,
        }
    }

    /// How long a browser that has lost the stream waits before calling the
    /// shell unreachable.
    ///
    /// Derived from `staleness_ms` rather than set separately, and floored, for
    /// one reason: these are two halves of the same judgement. The shell decides
    /// when a platform's data has aged; the browser decides when the shell's
    /// absence has. A shell tuned for live data marked a panel stale after two
    /// seconds while the browser waited thirty to say anything about the shell,
    /// and a person watching had no way to know why the two behaved so
    /// differently.
    ///
    /// Bounded at both ends, because the two judgements are related without
    /// being the same. Staleness is about a platform's data aging, and ninety
    /// seconds of that is reasonable for a panel refreshed every thirty. Stream
    /// loss is about the shell being gone, and ninety seconds of a person
    /// staring at a dashboard before anything says so is not.
    ///
    /// The floor is because an `EventSource` reconnects on its own within a few
    /// seconds, and a shorter grace would report a shell as gone during an
    /// ordinary reconnect. The ceiling is a person's patience.
    pub fn stream_loss_grace(&self) -> chrono::Duration {
        let seconds = (self.staleness_ms / 1000).clamp(5, 30);
        chrono::Duration::seconds(seconds as i64)
    }

    /// How often the driver should look for work, given that policy.
    ///
    /// The driver used to tick at a flat hundred milliseconds, which quantised
    /// every refresh up to the next tenth of a second: a 125ms refresh fired at
    /// 200ms, and 10Hz was the ceiling no matter what was asked for. Deriving
    /// the tick from the interval removes both the ceiling and the rounding,
    /// and the floor keeps a misconfiguration from becoming a spin loop.
    pub fn tick(&self) -> Duration {
        Duration::from_millis((self.refresh_ms / 4).clamp(5, 100))
    }
}

/// Why a configuration cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The authenticator cannot be used as configured.
    #[error("authentication: {0}")]
    Authenticator(String),

    /// A named environment variable is not in the environment.
    #[error("{setting} names `{variable}`, which is not in the environment")]
    MissingVariable {
        /// Which setting named it.
        setting: &'static str,
        /// The variable it named.
        variable: String,
    },

    /// The file could not be read.
    #[error("could not read {path}: {reason}")]
    Unreadable {
        /// Which file.
        path: String,
        /// Why.
        reason: String,
    },
    /// The file is not valid TOML, or not shaped like a configuration.
    #[error("could not parse {path}: {reason}")]
    Malformed {
        /// Which file.
        path: String,
        /// Why.
        reason: String,
    },
    /// A rule that would otherwise fail at the first request.
    #[error("platform `{platform}`: {reason}")]
    Platform {
        /// Which platform.
        platform: String,
        /// What is wrong.
        reason: String,
    },
    /// Two platforms claim one identity.
    #[error("platform id `{0}` is used more than once")]
    DuplicatePlatform(String),
}

impl Config {
    /// The connection string, from wherever it was configured.
    pub fn database_url(&self) -> Option<String> {
        match &self.database_url_env {
            Some(named) => std::env::var(named).ok(),
            None => self.database_url.clone(),
        }
    }

    /// Read a configuration and check it.
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|error| ConfigError::Unreadable {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;

        let config: Self = toml::from_str(&text).map_err(|error| ConfigError::Malformed {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;

        config.check()?;
        Ok(config)
    }

    /// Every rule that can be checked before a single request is made.
    ///
    /// An operator should learn about a misconfiguration when they deploy, not
    /// when a viewer opens a panel (HLIN-S-0005 NFR-1.2).
    pub fn check(&self) -> Result<(), ConfigError> {
        // How a person becomes a principal, first. Everything below is about
        // how the shell talks to platforms; this is about whether it knows who
        // is asking at all, and a shell that cannot answer that should refuse
        // to start rather than serve every request as the same person.
        self.auth.check().map_err(ConfigError::Authenticator)?;

        // A named variable that is not there is refused rather than ignored.
        // Ignoring it leaves the shell running with no database, which looks
        // exactly like never having configured one — so a deployment that
        // mounted the secret wrong finds out weeks later, from the contract
        // violation nobody caught across a restart.
        if let Some(named) = &self.database_url_env
            && std::env::var(named).is_err()
        {
            return Err(ConfigError::MissingVariable {
                setting: "database_url_env",
                variable: named.clone(),
            });
        }

        let mut seen: Vec<&str> = Vec::new();

        for platform in &self.platforms {
            if seen.contains(&platform.id.as_str()) {
                return Err(ConfigError::DuplicatePlatform(platform.id.clone()));
            }
            seen.push(&platform.id);

            if !hlin_manifest::validate::is_valid_key(&platform.id) {
                return Err(ConfigError::Platform {
                    platform: platform.id.clone(),
                    reason: "is not a usable platform id".to_string(),
                });
            }

            platform
                .auth
                .check(&platform.id, &platform.base_url, self.port)
                .map_err(|reason| ConfigError::Platform {
                    platform: platform.id.clone(),
                    reason,
                })?;
        }

        Ok(())
    }

    /// Who is making this request.
    ///
    /// Takes the headers rather than reading a fixed value out of
    /// configuration, which is the whole of the change: every handler used to
    /// call a no-argument `principal()` and get the same person for every
    /// request, so ownership, visibility and per-viewer deduplication were
    /// implemented, tested, and unable to do anything.
    ///
    /// [`Asking::Nobody`] means the request carries no principal and must be
    /// refused. It is deliberately not "fall back to the configured one": a
    /// strategy that silently degraded to `dev` when a header was missing would
    /// be a strategy nobody could tell was broken.
    ///
    /// The third answer is what `oidc` needed and what an `Option` could not
    /// express. A session cookie is not an identity — it is a name for one the
    /// shell wrote down — so answering that question means a database read, and
    /// this function cannot do one. Returning `None` for it would have been the
    /// worst of both: the code would compile, and every request under `oidc`
    /// would be refused.
    pub fn principal_from(&self, headers: &axum::http::HeaderMap) -> Asking {
        match &self.auth {
            AuthConfig::Dev { sub, name, groups } => {
                let mut principal = hlin_identity::Principal::new(sub.clone());
                principal.name = name.clone();
                principal.groups = groups.clone();
                Asking::Known(principal)
            }

            AuthConfig::TrustedHeader {
                header,
                groups_header,
                name_header,
                ..
            } => {
                let read = |name: &str| -> Option<String> {
                    headers
                        .get(name)
                        .and_then(|value| value.to_str().ok())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };

                let Some(sub) = read(header) else {
                    return Asking::Nobody;
                };
                let mut principal = hlin_identity::Principal::new(sub);
                principal.name = name_header.as_deref().and_then(read);
                principal.groups = groups_header
                    .as_deref()
                    .and_then(read)
                    .map(|value| {
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Asking::Known(principal)
            }

            AuthConfig::Oidc(oidc) => match session_cookie(headers, &oidc.cookie.name) {
                Some(value) => Asking::Session(value),
                None => Asking::Nobody,
            },
        }
    }
}

/// What the headers alone can say about who is asking.
#[derive(Debug, Clone, PartialEq)]
pub enum Asking {
    /// This is who, and the headers were enough to know it.
    Known(hlin_identity::Principal),

    /// Nobody. The request must be refused.
    Nobody,

    /// A session the shell issued, named by the cookie's value.
    ///
    /// Not an identity yet: the value has to be looked up, which is a read the
    /// caller can do and this cannot.
    Session(String),
}

impl Asking {
    /// The principal, where the headers alone were enough to know one.
    ///
    /// A session answers `None` here, because it is not an answer yet — which
    /// is why this is not the extractor's path and is named for what it does
    /// rather than called `principal`.
    pub fn known(self) -> Option<hlin_identity::Principal> {
        match self {
            Self::Known(principal) => Some(principal),
            Self::Nobody | Self::Session(_) => None,
        }
    }
}

/// The named cookie's value, from a `Cookie` header.
fn session_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are the intervals the specifications name, so a shell with
    /// no `[timings]` at all behaves as it did before any of this was settable.
    #[test]
    fn the_defaults_are_the_documented_intervals() {
        let policy = Timings::default().policy();

        assert_eq!(policy.refresh, chrono::Duration::seconds(30));
        assert_eq!(policy.staleness, chrono::Duration::seconds(90));
        assert_eq!(policy.retry_from, chrono::Duration::seconds(30));
        assert_eq!(policy.retry_ceiling, chrono::Duration::seconds(300));
    }

    /// A configuration that omits the new keys still parses, which is what
    /// keeps every existing `hlin.toml` working.
    #[test]
    fn a_configuration_without_them_still_parses() {
        let timings: Timings = toml::from_str("poll_seconds = 5").expect("it parses");

        assert_eq!(timings.poll_seconds, 5);
        assert_eq!(timings.refresh_ms, 30_000);
    }

    /// The tick has to be finer than the refresh, or the driver rounds every
    /// interval up to its own period: the bug this replaced was a flat 100ms
    /// tick turning a 125ms refresh into a 200ms one.
    #[test]
    fn the_tick_is_finer_than_the_refresh_it_serves() {
        let timings = Timings {
            refresh_ms: 125,
            ..Default::default()
        };

        assert_eq!(timings.tick(), Duration::from_millis(31));
        assert!(timings.tick().as_millis() * 2 < u128::from(timings.refresh_ms));
    }

    /// Fast and slow both stay in range: a slow shell does not wake up every
    /// eight seconds to find nothing, and a misconfigured fast one cannot turn
    /// the driver into a spin loop.
    #[test]
    fn the_tick_is_bounded_at_both_ends() {
        let tick = |refresh_ms| {
            Timings {
                refresh_ms,
                ..Default::default()
            }
            .tick()
        };

        assert_eq!(tick(30_000), Duration::from_millis(100), "capped when slow");
        assert_eq!(tick(1), Duration::from_millis(5), "floored when fast");
        assert_eq!(tick(0), Duration::from_millis(5), "and when nonsensical");
    }
}

#[cfg(test)]
mod grace_tests {
    use super::*;

    /// The two halves of one judgement. The shell decides when a platform's
    /// data has aged; the browser decides when the shell's absence has. A shell
    /// tuned for live data used to mark a panel stale after two seconds while
    /// the browser waited a hardcoded thirty to say anything about the shell.
    #[test]
    fn the_browsers_grace_follows_the_shells_staleness() {
        let slow = Timings {
            staleness_ms: 90_000,
            ..Default::default()
        };
        assert_eq!(
            slow.stream_loss_grace(),
            chrono::Duration::seconds(30),
            "capped: ninety seconds of a person staring at a dashboard before \
             anything says the shell is gone is not a grace, it is a hang"
        );

        let live = Timings {
            staleness_ms: 20_000,
            ..Default::default()
        };
        assert_eq!(
            live.stream_loss_grace(),
            chrono::Duration::seconds(20),
            "between the bounds it follows the shell's own sense of current"
        );
    }

    /// Floored, because the browser is judging a network rather than a
    /// platform: an `EventSource` reconnects on its own within a few seconds,
    /// and a shorter grace would report the shell as gone during an ordinary
    /// reconnect.
    #[test]
    fn the_grace_never_drops_below_a_reconnect() {
        for staleness_ms in [0, 500, 2_000, 4_999] {
            let timings = Timings {
                staleness_ms,
                ..Default::default()
            };
            assert_eq!(
                timings.stream_loss_grace(),
                chrono::Duration::seconds(5),
                "a {staleness_ms}ms staleness must still leave room to reconnect"
            );
        }
    }
}
