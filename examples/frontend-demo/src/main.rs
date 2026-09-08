//! Hlin's frontend, drawn by the demo design pack.
//!
//! The whole of a Hlin front end: pick a pack, mount the app. Everything else
//! — the picker, the grid, the stream, the time controls — is `hlin-ui`, which
//! has no idea what is drawing its panels.
//!
//! This one exists so the repository can demonstrate itself while depending on
//! nothing published. A front end built on a real design system looks exactly
//! the same, with a different name on one line.

use hlin_pack_demo::DemoPack;
use hlin_ui::app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| view! { <App pack=DemoPack /> });
}
