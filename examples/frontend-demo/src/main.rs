//! Hlin's frontend, drawn by the demo design pack.
//!
//! The whole of a Hlin front end: pick a pack, mount the app. Everything else
//! — the picker, the grid, the stream, the time controls — is `hlin-ui`, which
//! has no idea what is drawing its panels.
//!
//! This one exists so the repository can demonstrate itself while depending on
//! nothing published. A front end built on a real design system looks exactly
//! the same, with a different name on one line.
//!
//! It also registers one view in code, `about`, which a content component on a
//! surface opens with `{ "view": "about" }`.

use hlin_pack_demo::DemoPack;
use hlin_ui::Views;
use hlin_ui::app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    let views = Views::new().with("about", || {
        view! {
            <article class="demo-about">
                <h2>"About this shell"</h2>
                <p>"A view written in the front end's own code, opened in a content component."</p>
            </article>
        }
    });
    leptos::mount::mount_to_body(move || view! { <App pack=DemoPack views=views.clone() /> });
}
