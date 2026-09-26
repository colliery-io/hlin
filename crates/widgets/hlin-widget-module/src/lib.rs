//! A widget's Hlin module: its components, mounted with the Hlin client
//! ([[HLIN-I-0013]]).
//!
//! A widget's module is a re-export of its components crate and one call:
//!
//! ```text
//! pub use hlin_widget_counter_components::*;
//!
//! fn main() {
//!     hlin_widget_module::mount("counter", Counter);
//! }
//! ```
//!
//! [`mount`] connects to the shell, waits for `init`, mounts the components
//! with [`Hlin`] as their client, and says `ready`. [`Hlin`] is the
//! `hlin_widget_ui::Client` over the SDK's bridge: every request goes through
//! the shell, which adds who is asking and sends it to the platform's `/hlin`
//! subtree; `changed` for this panel, or a new context, is a change; a write
//! that happened is announced, so the platform's other modules on the page
//! fetch again; the shell's refusals are said in words for a person
//! ([`shell_words`]), never its developer-facing reason.
//!
//! The components, the handle they use and the stylesheet are
//! `hlin-widget-ui`'s, shared with the widget's own UI.
//!
//! The rest of this crate ([`start`], [`Widget`], [`loaded_view`]) is the
//! shape a module had before, when it had its own UI. It stays until the last
//! widget is converted ([[HLIN-T-0096]], [[HLIN-T-0097]]).

mod hlin;
mod legacy;
mod words;

pub use hlin::{Hlin, mount};
pub use hlin_widget_ui::{Loaded, STYLE};
pub use legacy::{Widget, loaded_view, start};
pub use words::{outcome, platform_words, shell_words, worth_retrying};

/// What the shell logs as the kit a widget was drawn with, so drift between
/// widgets built against different versions of this crate can be seen.
pub const KIT: &str = concat!("hlin-widget@", env!("CARGO_PKG_VERSION"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kit_names_this_crate_and_its_version() {
        assert!(KIT.starts_with("hlin-widget@"));
    }
}
