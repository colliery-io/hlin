use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use hlin::config::Config;
use hlin::manifest_client::HttpManifestClient;
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{MemoryStore, PostgresStore, Store};

#[derive(Parser)]
#[command(name = "hlin", version, about = hlin::TAGLINE)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Display version information
    Version,

    /// Run the shell.
    Serve {
        /// Where the configuration lives.
        #[arg(long, default_value = "demo/hlin.toml")]
        config: PathBuf,
    },

    /// Read a configuration and say what is wrong with it, without starting.
    Check {
        /// Where the configuration lives.
        #[arg(long, default_value = "demo/hlin.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hlin=debug")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { config }) => serve(&config).await,

        Some(Commands::Check { config }) => {
            let config = Config::load(&config)?;
            println!(
                "configuration is usable: {} platform(s)",
                config.platforms.len()
            );
            for platform in &config.platforms {
                println!(
                    "  {} at {} via {}",
                    platform.id,
                    platform.base_url,
                    platform.auth.name()
                );
            }
            Ok(())
        }

        Some(Commands::Version) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }

        None => {
            println!("Hlin v{}", env!("CARGO_PKG_VERSION"));
            println!("{}", hlin::TAGLINE);
            println!("Run with --help for usage information.");
            Ok(())
        }
    }
}

async fn serve(config_path: &std::path::Path) -> anyhow::Result<()> {
    let config = Arc::new(Config::load(config_path)?);

    // The key is loaded rather than generated so a restart keeps its `kid` and
    // platforms are not made to refetch for nothing.
    let issuer = Arc::new(hlin_identity::Issuer::load_or_generate(
        &config.issuer,
        &config.key_path,
    )?);
    tracing::info!(
        issuer = config.issuer,
        kid = issuer.kid(),
        "signing key ready"
    );

    let store: Arc<dyn Store> = match &config.database_url {
        Some(url) => {
            let store = PostgresStore::connect(url).await?;
            tracing::info!("connected to Postgres and applied migrations");
            Arc::new(store)
        }
        None => {
            // Contract memory is what makes runtime enforcement survive a
            // restart, so running without it is a real limitation rather than
            // a configuration detail.
            tracing::warn!(
                "no database configured: contract memory will not survive a restart, so a \
                 breaking change shipped across one will go unnoticed"
            );
            Arc::new(MemoryStore::new())
        }
    };

    let client = Arc::new(
        HttpManifestClient::new(config.timings.upstream_timeout())
            .map_err(|reason| anyhow::anyhow!("could not build the manifest client: {reason}"))?,
    );
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), client)
            .map_err(|reason| anyhow::anyhow!("{reason}"))?,
    );

    match registry.restore().await {
        Ok(0) => tracing::info!("no contracts remembered from a previous run"),
        Ok(count) => tracing::info!(platforms = count, "restored remembered contracts"),
        Err(error) => tracing::warn!(%error, "could not restore remembered contracts"),
    }

    // One task per platform would be tidier still; one loop over all of them is
    // enough while the fetch itself has a timeout, and it keeps the ordering of
    // log lines readable during a demo.
    let polling = registry.clone();
    let interval = config.timings.poll();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            polling.poll_all().await;
        }
    });

    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store,
        client: reqwest::Client::builder()
            .timeout(config.timings.upstream_timeout())
            .build()
            .unwrap_or_default(),
        stream_client: reqwest::Client::builder()
            .connect_timeout(config.timings.upstream_timeout())
            .build()
            .unwrap_or_default(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    // Housekeeping for the strategy that issues sessions. Expiry is enforced
    // when a session is read, so this is only about the tables not growing
    // forever; it costs nothing to start where there is nothing to sweep.
    hlin::auth::sweep(state.store.clone());

    let listener = tokio::net::TcpListener::bind((config.bind.as_str(), config.port)).await?;
    tracing::info!(
        platforms = config.platforms.len(),
        "listening on http://{}:{}",
        config.bind,
        config.port
    );

    // The frontend, where it has been built. A missing bundle is a warning
    // rather than a failure: the API is useful on its own, and `angreal ui
    // build` is a separate step a reader may not have run.
    let assets = config.frontend.clone();
    let app = if assets.is_dir() {
        tracing::info!("serving the frontend from {}", assets.display());
        let index = assets.join("index.html");
        router(state).fallback_service(
            tower_http::services::ServeDir::new(&assets)
                .fallback(tower_http::services::ServeFile::new(index)),
        )
    } else {
        tracing::warn!(
            "no frontend bundle at {}; the API is up but there is nothing to look at. \
             Run `angreal ui build`",
            assets.display()
        );
        router(state)
    };

    axum::serve(listener, app).await?;
    Ok(())
}
