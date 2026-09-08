//! Hlin's frontend, drawn by Colliery's Aurora Dark.
//!
//! The same binary as `frontend-demo` with one name changed, which is the
//! claim this whole example exists to make. Choosing a design system is a
//! dependency and one identifier; everything else — the picker, the grid, the
//! stream, the time controls — is `hlin-ui`, and it has no idea what is drawing
//! its panels.

use aurora_leptos::AuroraPack;
use hlin_ui::app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| view! { <App pack=AuroraPack /> });
}
