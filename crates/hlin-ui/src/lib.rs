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
