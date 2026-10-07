//! Whose name the page wears.
//!
//! The operator's, from `/api/config` (`brand.name`), or Hlin's until the
//! shell says otherwise. Held here rather than threaded through every
//! component and state transition that writes a sentence naming the product,
//! because there is one page and one name for its life.
//!
//! The colours are not here. They come in the stylesheet the app links at
//! [`STYLESHEET_PATH`], after the pack's, and nothing in this crate reads them.

use std::cell::RefCell;

/// The operator's stylesheet, served by the shell: empty when there is none,
/// so it is linked always.
pub const STYLESHEET_PATH: &str = "/brand/style.css";

/// The operator's logo, where `brand.logo` says there is one.
pub const LOGO_PATH: &str = "/brand/logo";

/// The name when nobody configured another.
pub const DEFAULT_NAME: &str = "Hlin";

thread_local! {
    static NAME: RefCell<String> = RefCell::new(DEFAULT_NAME.to_string());
}

/// The product's name, for a sentence that names it.
pub fn name() -> String {
    NAME.with(|name| name.borrow().clone())
}

/// What the shell said the name is.
pub fn set_name(name: &str) {
    NAME.with(|current| *current.borrow_mut() = name.to_string());
}
