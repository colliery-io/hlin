//! The converter's server: only what the shell needs to host its module.
//! The flags are the ones every widget takes (`hlin_widget_support::Common`).

use hlin_widget_converter::{api, widget};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    hlin_widget_support::run(widget(), (), api()).await
}
