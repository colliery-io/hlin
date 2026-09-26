//! Starting a widget: the flags every widget takes, and `main` in one call.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::dist::{Builds, Dist};
use crate::files::ModuleFiles;
use crate::platform::Platform;
use crate::site::{HLIN_BASE, Site, site};
use crate::widget::Widget;

/// The flags every widget takes.
///
/// A widget with flags of its own flattens this into its own `Parser` and
/// calls [`serve`]; one without calls [`run`] or [`run_with`], which parse
/// these alone.
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

    /// Where Hlin's surface is served: the manifest, the module, the API
    /// Hlin calls and the event stream. The shell's `base_url` for this
    /// platform ends in it.
    #[arg(long, default_value = HLIN_BASE)]
    pub hlin_base: String,

    /// DEMO ONLY: answer the widget's own `/api/` as this one person, with
    /// no sign-in at all, so its own UI at `/` works. Off by default, which
    /// refuses `/api/`. Never on a widget anyone else can reach.
    #[arg(long)]
    pub local_user: Option<String>,

    /// Where the built module is: Trunk's output for the widget's `module/`.
    ///
    /// Read once, at start. Without it the widget reads the build it was
    /// compiled with, or the one in this repository; without any, it still
    /// runs, and the shell draws its fallback, or says the panel cannot be
    /// shown.
    #[arg(long)]
    pub module_dir: Option<PathBuf>,

    /// Where the widget's own UI is built: Trunk's output for its `ui/`.
    /// Read once, at start, and chosen as `--module-dir` is.
    #[arg(long)]
    pub ui_dir: Option<PathBuf>,
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    common: Common,
}

/// The whole `main` of a widget with no UI of its own yet: parse [`Common`],
/// then [`serve`] its module from `widget.built`.
pub async fn run<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
) -> anyhow::Result<()> {
    let command = Cli::command().name(widget.name).about(widget.description);
    let cli = Cli::from_arg_matches(&command.get_matches())?;
    serve(cli.common, widget, state, api, None).await
}

/// The whole `main` of a widget with its own UI: parse [`Common`], then
/// [`serve`] both builds.
///
/// ```text
/// hlin_widget_support::run_with(widget(), Counter::default(), api(), Builds {
///     ui: hlin_widget_support::dist!("ui/dist"),
///     module: hlin_widget_support::dist!("module/dist"),
/// })
/// ```
pub async fn run_with<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
    builds: Builds,
) -> anyhow::Result<()> {
    let command = Cli::command().name(widget.name).about(widget.description);
    let cli = Cli::from_arg_matches(&command.get_matches())?;
    serve(cli.common, widget, state, api, Some(builds)).await
}

/// Check the widget's manifest, read its builds, and serve it until stopped.
///
/// `builds` is `None` for a widget with no UI of its own yet, whose module is
/// at `widget.built`.
pub async fn serve<S: Send + 'static>(
    common: Common,
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
    builds: Option<Builds>,
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
    if let Some(defect) = Site::base_defect(&common.hlin_base) {
        anyhow::bail!("--hlin-base {:?}: {defect}", common.hlin_base);
    }

    let url = common
        .shell_keys
        .unwrap_or_else(|| format!("http://127.0.0.1:8080/{}", hlin_identity::JWKS_PATH));
    let verifier = Arc::new(hlin_identity::Verifier::fetching(
        &common.shell_issuer,
        Box::new(HttpJwks { url }),
    ));

    let module_dist = builds.map_or(Dist::at(widget.built), |builds| builds.module);
    let module = match module_dist.files(common.module_dir.as_deref()) {
        Some((files, from)) => {
            tracing::info!(from, "serving the widget's module");
            files
        }
        None => {
            tracing::warn!(
                dir = module_dist.dir,
                "no module built, so the shell will draw the fallback if there is one; \
                 `trunk build` in the widget's module/ builds it"
            );
            ModuleFiles::none()
        }
    };
    let ui = builds.and_then(|builds| match builds.ui.files(common.ui_dir.as_deref()) {
        Some((files, from)) => {
            tracing::info!(from, "serving the widget's own UI");
            Some(files)
        }
        None => {
            tracing::warn!(
                dir = builds.ui.dir,
                "no UI built, so / says so; `trunk build` in the widget's ui/ builds it"
            );
            None
        }
    });

    if let Some(name) = &common.local_user {
        tracing::warn!(
            local_user = name,
            "DEMO ONLY: /api/ answers every request as {name}, with no sign-in. \
             Nobody else must be able to reach this widget."
        );
    }

    let listener = tokio::net::TcpListener::bind((common.bind.as_str(), common.port)).await?;
    tracing::info!(
        platform = id,
        hlin = common.hlin_base,
        "listening on http://{}:{}",
        common.bind,
        common.port
    );

    let platform = Platform::new(id, widget, state, verifier, module);
    let layout = Site {
        hlin_base: common.hlin_base,
        local_user: common.local_user,
        ui,
    };
    axum::serve(listener, site(platform, api, layout)).await?;
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
