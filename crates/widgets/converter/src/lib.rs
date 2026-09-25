//! Widget 19 of twenty ([[HLIN-I-0012]]): a unit converter, entirely local.
//!
//! Everything this widget does happens in its module: the units, the
//! arithmetic and what the person typed. The module makes no requests at all,
//! so this server is only what the shell needs to host it: the manifest, the
//! module's files, health and the (silent) event stream, all from
//! `hlin_widget_support::router`. It has no routes of its own and no state.
//!
//! **No fallback**, and on purpose. A shell-drawn fallback is data the
//! platform serves in an envelope, for the shell to draw when the module
//! cannot load. A converter has no data: its answer is whatever the person
//! types, worked out as they type it, and a `stat` or `table` of nothing would
//! be a panel pretending to work. Where the module cannot load, the shell
//! says the panel cannot be shown, which is the truth.
//!
//! Not shared: nothing is kept, so there is nothing to share. The module keeps
//! what the person typed across being scrolled away and back through the
//! bridge's `suspend` and `restored` state, which stays in the page.

use axum::Router;
use hlin_widget_support::{Platform, Widget};

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "converter";

/// What the shell is told about this widget.
pub fn widget() -> Widget<()> {
    Widget {
        panel: PANEL,
        name: "Converter",
        icon: "calculator",
        title: "Unit converter",
        description: "Converts units in your browser; nothing is sent anywhere, so there is \
                      no fallback: without its module there is nothing to show",
        shared: false,
        // See the crate docs: a converter has no data for the shell to draw.
        fallback: None,
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// This widget's own routes: none. The module asks for nothing.
pub fn api() -> Router<Platform<()>> {
    Router::new()
}
