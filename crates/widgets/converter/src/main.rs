//! The converter's server: its own UI, and what the shell needs to host its
//! module.
//! The flags are the ones every widget takes (`hlin_widget_support::Common`).

use hlin_widget_converter::{api, widget};
use hlin_widget_support::{Builds, dist};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let builds = Builds {
        ui: dist!("ui/dist"),
        module: dist!("module/dist"),
    };
    hlin_widget_support::run_with(widget(), (), api(), builds).await
}
