use std::sync::Arc;

use clap::Parser;
use hlin_identity::extract::IdentityState;
use hlin_sample_checklist::lists::Lists;
use hlin_sample_checklist::module::ModuleFiles;
use hlin_sample_checklist::{App, router};

#[derive(Parser)]
#[command(
    name = "hlin-sample-checklist",
    version,
    about = "A reference platform for Hlin that accepts writes: shared checklists"
)]
struct Cli {
    /// The platform's id: its `platform.id`, and the audience every token
    /// must name.
    #[arg(long, default_value = "checklist")]
    name: String,

    /// Port to listen on.
    #[arg(long, default_value_t = 8083)]
    port: u16,

    /// Which address to listen on.
    ///
    /// Loopback by default, as `hlin-sample-platform` is and for its reason; a
    /// container has to set this or nothing can reach it.
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Where the shell publishes the keys that verify its tokens.
    #[arg(long)]
    shell_keys: Option<String>,

    /// The issuer name every token must claim.
    #[arg(long, default_value = "hlin")]
    shell_issuer: String,

    /// Where the built module is: Trunk's output for `module/`.
    ///
    /// Read once, at start. Without it the platform still runs, and the shell
    /// draws the `items` panel as its table fallback.
    #[arg(long, default_value = hlin_sample_checklist::module::BUILT)]
    module_dir: std::path::PathBuf,
}

/// Fetches the shell's key set over HTTP, as `hlin-sample-platform` does.
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

    // A reference that is wrong is worse than none, so refuse to start rather
    // than serve a manifest the shell would reject.
    let document = hlin_sample_checklist::manifest::build(&cli.name);
    let checked = hlin_manifest::validate(&document, &cli.name);
    if let Some(defect) = &checked.document {
        anyhow::bail!("this platform's own manifest is malformed: {defect}");
    }
    if !checked.rejected().is_empty() || !checked.unusable_routes.is_empty() {
        anyhow::bail!("this platform's own manifest declares something the shell would reject");
    }

    let keys = cli
        .shell_keys
        .clone()
        .unwrap_or_else(|| format!("http://127.0.0.1:8080/{}", hlin_identity::JWKS_PATH));

    // Unlike the read-only sample there is no `--auth open`: every rule here
    // is about who is asking, and an anonymous caller would be on no list.
    // Driving it without a shell is the `dev-identity` feature's job.
    let identity = IdentityState {
        verifier: Arc::new(hlin_identity::Verifier::fetching(
            &cli.shell_issuer,
            Box::new(HttpJwks { url: keys }),
        )),
        audience: cli.name.clone(),
        #[cfg(feature = "dev-identity")]
        development_principal: hlin_identity::Principal {
            sub: "dev-alice".to_string(),
            name: Some("Alice".to_string()),
            email: Some("alice@example.com".to_string()),
            groups: vec![],
        },
    };

    let module = match ModuleFiles::read(&cli.module_dir) {
        Ok(files) if files.has_entry() => {
            tracing::info!(dir = %cli.module_dir.display(), "serving the checklist's module");
            files
        }
        _ => {
            tracing::warn!(
                dir = %cli.module_dir.display(),
                "no module built here, so the shell will draw the list as a table; \
                 `trunk build` in crates/hlin-sample-checklist/module builds it"
            );
            ModuleFiles::none()
        }
    };

    let app = App::new(cli.name.clone(), identity, Lists::seeded()).with_module(module);

    let listener = tokio::net::TcpListener::bind((cli.bind.as_str(), cli.port)).await?;
    tracing::info!(
        platform = cli.name,
        "listening on http://{}:{}; state is in memory and a restart empties it",
        cli.bind,
        cli.port
    );

    axum::serve(listener, router(app)).await?;
    Ok(())
}
