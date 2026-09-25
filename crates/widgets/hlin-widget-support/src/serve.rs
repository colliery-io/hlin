//! Starting a widget: the flags every widget takes, and `main` in one call.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::files::ModuleFiles;
use crate::platform::{Platform, router};
use crate::widget::Widget;

/// The flags every widget takes.
///
/// A widget with flags of its own flattens this into its own `Parser` and
/// calls [`serve`]; one without calls [`run`], which parses these alone.
#[derive(clap::Args, Debug, Clone)]
pub struct Common {
    /// The platform's id, which is also its `platform.id` and the audience
    /// every token must name. Defaults to the widget's panel key.
    #[arg(long)]
    pub name: Option<String>,

    /// Port to listen on. No default: `angreal demo up --with twenty` gives
    /// each widget its own, from the list it reads.
    #[arg(long)]
    pub port: u16,

    /// Which address to listen on.
    ///
    /// Loopback by default, so a laptop does not put this on the network by
    /// accident.
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,

    /// Where the shell publishes the keys that verify its tokens.
    #[arg(long)]
    pub shell_keys: Option<String>,

    /// The issuer name every token must claim.
    #[arg(long, default_value = "hlin")]
    pub shell_issuer: String,

    /// Where the built module is: Trunk's output for the widget's `module/`.
    ///
    /// Read once, at start. Without it the widget still runs, and the shell
    /// draws its fallback, or says the panel cannot be shown.
    #[arg(long)]
    pub module_dir: Option<PathBuf>,
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    common: Common,
}

/// A widget's whole `main`: parse [`Common`], then [`serve`].
pub async fn run<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
) -> anyhow::Result<()> {
    let command = Cli::command().name(widget.name).about(widget.description);
    let cli = Cli::from_arg_matches(&command.get_matches())?;
    serve(cli.common, widget, state, api).await
}

/// Check the widget's manifest, read its module, and serve it until stopped.
pub async fn serve<S: Send + 'static>(
    common: Common,
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let id = common.name.unwrap_or_else(|| widget.panel.to_string());

    // A reference that is wrong is worse than none, so refuse to start rather
    // than serve a manifest the shell would reject or partly ignore.
    if let Some(defect) = widget.defects(&id) {
        anyhow::bail!("this widget's own manifest is one the shell would refuse: {defect}");
    }

    let url = common
        .shell_keys
        .unwrap_or_else(|| format!("http://127.0.0.1:8080/{}", hlin_identity::JWKS_PATH));
    let verifier = Arc::new(hlin_identity::Verifier::fetching(
        &common.shell_issuer,
        Box::new(HttpJwks { url }),
    ));

    let dir = common
        .module_dir
        .unwrap_or_else(|| PathBuf::from(widget.built));
    let module = match ModuleFiles::read(&dir) {
        Ok(files) if files.has_entry() => {
            tracing::info!(dir = %dir.display(), "serving the widget's module");
            files
        }
        _ => {
            tracing::warn!(
                dir = %dir.display(),
                "no module built here, so the shell will draw the fallback if there is one; \
                 `trunk build` in the widget's module/ builds it"
            );
            ModuleFiles::none()
        }
    };

    let listener = tokio::net::TcpListener::bind((common.bind.as_str(), common.port)).await?;
    tracing::info!(
        platform = id,
        "listening on http://{}:{}",
        common.bind,
        common.port
    );

    let platform = Platform::new(id, widget, state, verifier, module);
    axum::serve(listener, router(platform, api)).await?;
    Ok(())
}

/// Fetches the shell's key set over HTTP.
///
/// A platform brings its own client; the identity crate only asks for the
/// document. `curl`, as the sample platforms do: blocking, because it is
/// called rarely and from a context that can afford to wait, and it keeps an
/// HTTP client out of twenty binaries that otherwise need none.
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
