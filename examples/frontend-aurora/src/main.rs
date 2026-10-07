//! Hlin's frontend, drawn by Colliery's Aurora.
//!
//! The same binary as `frontend-demo` with one name changed, which is the
//! claim this whole example exists to make. Choosing a design system is a
//! dependency and one identifier; everything else — the picker, the grid, the
//! stream, the time controls — is `hlin-ui`, and it has no idea what is drawing
//! its panels.
//!
//! The one addition is Aurora's own: its Light / Dark / System switch, in the
//! bar. The shell draws it where it is told, and sends every module the
//! colours the switch leaves on the page.

use aurora_leptos::{AuroraPack, ThemeToggle};
use hlin_ui::app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| {
        view! { <App pack=AuroraPack bar=|| view! { <ThemeToggle /> } /> }
    });
}

#[cfg(test)]
mod tests {
    /// `index.html` carries a copy of Aurora's script, because Trunk has no
    /// way to take it from the crate. A new Aurora that changes the script
    /// fails here rather than flashing the wrong theme.
    #[test]
    fn the_page_restores_the_theme_as_this_aurora_does() {
        assert!(include_str!("../index.html").contains(aurora_leptos::THEME_INIT_SCRIPT));
    }
}
