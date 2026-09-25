use std::sync::Arc;

use clap::Parser;
use hlin_sample_feed::module::ModuleFiles;
use hlin_sample_feed::{Config, router};

#[derive(Parser)]
#[command(
    name = "hlin-sample-feed",
    version,
    about = "A reference platform for Hlin that accepts writes: a feed of posts"
)]
struct Cli {
    /// The platform's id, which is also its `platform.id` and the audience
    /// every token must name.
    #[arg(long, default_value = "feed")]
    name: String,

    /// Port to listen on.
    #[arg(long, default_value_t = 8084)]
    port: u16,

    /// Which address to listen on.
    ///
    /// Loopback by default, so a laptop does not put this on the network by
    /// accident. A container has to set this, or it listens on its own
    /// loopback and nothing can reach it.
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// The email domain whose people may post.
    #[arg(long, default_value = "example.com")]
    domain: String,

    /// Somebody who may not post here, by email. Repeatable.
    ///
    /// A rule that lives nowhere but this platform: the shell has never heard
    /// of it, which is what makes it the example.
    #[arg(long = "muted", value_name = "EMAIL")]
    muted: Vec<String>,

    /// Where the shell publishes the keys that verify its tokens.
    #[arg(long)]
    shell_keys: Option<String>,

    /// The issuer name every token must claim.
    #[arg(long, default_value = "hlin")]
    shell_issuer: String,

    /// Where the built module is: Trunk's output for `module/`.
    ///
    /// Read once, at start. Without it the feed still runs, and the shell
    /// draws the `posts` panel as its table fallback.
    #[arg(long, default_value = hlin_sample_feed::module::BUILT)]
    module_dir: std::path::PathBuf,
}

/// Fetches the shell's key set over HTTP.
///
/// A platform brings its own client; the identity crate only asks for the
/// document. Blocking, because it is called rarely and from a context that
/// can afford to wait.
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
    // than serve a manifest the shell would reject or partly ignore.
    let document = hlin_sample_feed::manifest::build(&cli.name);
    let checked = hlin_manifest::validate(&document, &cli.name);
    if let Some(defect) = &checked.document {
        anyhow::bail!("this platform's own manifest is malformed: {defect}");
    }
    if !checked.rejected().is_empty() || !checked.unusable_routes.is_empty() {
        anyhow::bail!("this platform's own manifest declares something the shell would refuse");
    }

    let url = cli
        .shell_keys
        .clone()
        .unwrap_or_else(|| format!("http://127.0.0.1:8080/{}", hlin_identity::JWKS_PATH));
    let verifier = Arc::new(hlin_identity::Verifier::fetching(
        &cli.shell_issuer,
        Box::new(HttpJwks { url }),
    ));

    if cfg!(feature = "dev-identity") {
        tracing::warn!("built with dev-identity: an unsigned request is a development principal");
    }
    if !cli.muted.is_empty() {
        tracing::info!(muted = cli.muted.len(), "some people may not post here");
    }

    let module = match ModuleFiles::read(&cli.module_dir) {
        Ok(files) if files.has_entry() => {
            tracing::info!(dir = %cli.module_dir.display(), "serving the feed's module");
            files
        }
        _ => {
            tracing::warn!(
                dir = %cli.module_dir.display(),
                "no module built here, so the shell will draw the posts as a table; \
                 `trunk build` in crates/hlin-sample-feed/module builds it"
            );
            ModuleFiles::none()
        }
    };

    let config = Config {
        name: cli.name.clone(),
        verifier,
        rules: hlin_sample_feed::rules::Rules::new(&cli.domain, &cli.muted),
        posts: hlin_sample_feed::posts::seed(),
        module,
    };

    let listener = tokio::net::TcpListener::bind((cli.bind.as_str(), cli.port)).await?;
    tracing::info!(
        platform = cli.name,
        "listening on http://{}:{}",
        cli.bind,
        cli.port
    );

    axum::serve(listener, router(config)).await?;
    Ok(())
}
