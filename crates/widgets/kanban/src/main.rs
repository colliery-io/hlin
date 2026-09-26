//! The kanban widget's server. Everything it does is in the library; the
//! flags are the ones every widget takes (`hlin_widget_support::Common`).

use hlin_widget_kanban::{Column, api, widget};
use hlin_widget_support::{Builds, dist};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let builds = Builds {
        ui: dist!("ui/dist"),
        module: dist!("module/dist"),
    };
    hlin_widget_support::run_with(widget(), Column::seed(), api(), builds).await
}
