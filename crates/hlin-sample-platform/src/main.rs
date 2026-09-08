use clap::Parser;
use hlin_sample_platform::{Config, IdentityMode, router};

#[derive(Parser)]
#[command(
    name = "hlin-sample-platform",
    version,
    about = "A reference platform for Hlin: serves a manifest and synthetic panel data"
)]
struct Cli {
    /// The platform's id, which is also its `platform.id` in the manifest.
    #[arg(long, default_value = "orebank")]
    name: String,

    /// Port to listen on.
    #[arg(long, default_value_t = 8081)]
    port: u16,

    /// Serve a manifest that drops a panel without a major version bump, so
    /// the shell's contract enforcement has something to catch.
    #[arg(long)]
    breaking: bool,

    /// How callers are identified: `session`, `token`, or `open`.
    #[arg(long, default_value = "session")]
    auth: String,

    /// The cookie carrying the principal, when `--auth session`.
    #[arg(long, default_value = "hlin_demo_session")]
    session_cookie: String,

    /// A group required to read the worker-health panel, so a 403 is demoable.
    #[arg(long)]
    restrict_health_to: Option<String>,

    /// Where the shell publishes the keys that verify its tokens.
    #[arg(long)]
    shell_keys: Option<String>,

    /// The issuer name every token must claim.
    #[arg(long, default_value = "hlin")]
    shell_issuer: String,
}

/// Fetches the shell's key set over HTTP.
///
/// A platform brings its own client; the identity crate only asks for the
/// document. This one is blocking because it is called rarely and from a
/// context that can afford to wait.
struct HttpJwks {
    url: String,
}

impl hlin_identity::JwksFetcher for HttpJwks {
    fn fetch(&self) -> Result<hlin_identity::Jwks, String> {
        let body = std::process::Command::new("curl")
            .args(["-fsS", &self.url])
            .output()
            .map_err(|error| error.to_string())?;
        if !body.status.success() {
            return Err(format!("could not fetch {}", self.url));
        }
        serde_json::from_slice(&body.stdout).map_err(|error| error.to_string())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let identity = match cli.auth.as_str() {
        "session" => IdentityMode::Session {
            cookie: cli.session_cookie.clone(),
        },
        "token" => {
            let url = cli
                .shell_keys
                .clone()
                .unwrap_or_else(|| format!("http://127.0.0.1:8080/{}", hlin_identity::JWKS_PATH));
            IdentityMode::Token {
                verifier: std::sync::Arc::new(hlin_identity::Verifier::fetching(
                    &cli.shell_issuer,
                    Box::new(HttpJwks { url }),
                )),
            }
        }
        "open" => {
            tracing::warn!("running with --auth open; every caller is anonymous");
            IdentityMode::Open
        }
        other => anyhow::bail!("unknown --auth `{other}`; expected session, token or open"),
    };

    // A reference that is wrong is worse than none, so refuse to start rather
    // than serve a manifest the shell would reject.
    let document = hlin_sample_platform::manifest::build(&cli.name, cli.breaking);
    let checked = hlin_manifest::validate(&document, &cli.name);
    if let Some(defect) = &checked.document {
        anyhow::bail!("this platform's own manifest is malformed: {defect}");
    }
    if !checked.rejected().is_empty() {
        for (key, defect) in checked.rejected() {
            tracing::error!(panel = key, "own panel rejected: {defect}");
        }
        anyhow::bail!("this platform's own manifest declares panels the shell would reject");
    }

    if cli.breaking {
        tracing::warn!(
            "serving the breaking manifest: `{}` is gone and the contract version is unchanged",
            hlin_sample_platform::manifest::BREAKING_PANEL_KEY
        );
    }

    // The one piece of this platform that is not a function of the clock, and
    // the reason it has anything to report on its event stream.
    let changes = hlin_sample_platform::changes::Changes::new();
    hlin_sample_platform::changes::drive(changes.clone());

    let config = Config {
        name: cli.name.clone(),
        breaking: cli.breaking,
        identity,
        restricted_panel_group: cli.restrict_health_to.clone(),
        changes,
    };

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", cli.port)).await?;
    tracing::info!(
        platform = cli.name,
        panels = checked.accepted_keys().len(),
        "listening on http://127.0.0.1:{}",
        cli.port
    );

    axum::serve(listener, router(config)).await?;
    Ok(())
}
