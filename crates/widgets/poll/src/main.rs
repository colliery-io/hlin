//! The poll widget's server. Everything it does is in the library; the flags
//! are the ones every widget takes (`hlin_widget_support::Common`).

use hlin_widget_poll::{Poll, api, widget};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    hlin_widget_support::run(widget(), Poll::seed(), api()).await
}
