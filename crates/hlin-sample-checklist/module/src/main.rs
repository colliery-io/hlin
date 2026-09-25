//! The checklist's own module: the `items` panel as the checklist draws it.
//!
//! A reference for a platform team shipping its UI to Hlin. It is an ordinary
//! Leptos app with two differences, both handled by the SDK (`hlin-module`):
//! it waits for the shell's `init` before drawing, and every request goes
//! through the shell with `fetch` rather than to a URL of its own. It never
//! learns it is in a frame, holds no credential, and calls nothing but its own
//! platform's API.
//!
//! - [`api`]: the requests, and how their answers and refusals are read.
//! - [`app`]: what is drawn, and when the list is fetched again.
//!
//! Built with Trunk into `dist/`, which the platform serves under its
//! `assets` prefix; `angreal demo up --with collab` builds it.

mod api;
mod app;

use leptos::prelude::*;

/// The shared kit this module is drawn with, as the shell logs it.
const KIT: &str = "aurora@0.2";

fn main() {
    console_error_panic_hook::set_once();
    // Not Leptos's `spawn_local`: its executor starts when something is
    // mounted, and nothing is until `init` has arrived.
    wasm_bindgen_futures::spawn_local(async {
        // Resolves when the shell's `init` arrives, carrying the chosen list,
        // the theme and whether writes will be refused.
        let module = hlin_module::connect().await;
        let drawn = module.clone();
        leptos::mount::mount_to_body(move || view! { <app::Checklist module=drawn /> });
        // Drawn, so ready. The first answer from the platform comes later and
        // the module shows that it is loading meanwhile.
        module.ready(Some(KIT));
    });
}
