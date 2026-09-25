//! The dice widget's server. Everything it does is in the library; the
//! flags are the ones every widget takes (`hlin_widget_support::Common`).

use hlin_widget_dice::{Dice, api, widget};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    hlin_widget_support::run(widget(), Dice::default(), api()).await
}
