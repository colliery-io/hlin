//! Hlin's frontend, as a library.
//!
//! The composition machinery — the surface, the picker, the grid, the stream
//! client, the time controls — with no design system in it. A binary supplies a
//! pack and mounts [`app::App`]; this crate never learns which pack that was.
//!
//! ```ignore
//! use hlin_ui::app::App;
//! use my_design_system::MyPack;
//!
//! fn main() {
//!     leptos::mount::mount_to_body(|| leptos::view! { <App pack=MyPack /> });
//! }
//! ```
//!
//! Two halves live here for two different reasons. [`draft`], [`grid`] and
//! [`state`] are free of the DOM and are tested on the host, because the rules
//! that are easiest to get wrong — where a dragged panel lands, which frame is
//! stale, when a silent stream becomes an absent one — should not need a
//! browser to check. Everything else is wiring around them.

#![warn(missing_docs)]

/// The chrome this crate draws: the grid, the panel frames, the toolbar, the
/// states a panel can be in.
///
/// Exposed as a string, and injected by [`app::App`] itself, because the
/// alternative does not work outside this repository. The stylesheet ships in
/// the crate, but a consumer cannot link to a file inside their registry cache
/// — the examples here did it with a relative path into the source tree, which
/// is available to exactly nobody who takes this crate as a dependency. A front
/// end built by following the documentation therefore rendered every panel
/// unstyled and stacked in a column, with no sign of what was wrong.
///
/// A design pack brings its own stylesheet on top of this one; the two do not
/// overlap, because this is the chrome around a panel and a pack draws what is
/// inside it.
pub const APP_CSS: &str = include_str!("../app.css");

pub mod draft;
pub mod grid;
pub mod state;

// The browser half. Compiled everywhere, so a consumer building for `wasm32`
// gets it and the host build still type-checks what it can.
pub mod api;
pub mod app;
pub mod pack;
pub mod stream;

/// The stylesheet for the shell's own chrome: the bar, the picker, the grid.
///
/// Hlin's, not a pack's. A design pack styles panels; the frame around them is
/// this product's and ships with it.
pub const CHROME_CSS: &str = include_str!("../app.css");

pub use draft::LayoutDraft;
pub use pack::Drawer;
pub use state::{PanelView, SurfaceState};
