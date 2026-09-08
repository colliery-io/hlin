//! Hlin's frontend, holding two design systems and choosing between them when
//! the page loads.
//!
//! `?pack=aurora` draws with Colliery's Aurora Dark. `?pack=demo`, or nothing,
//! draws with the demo pack. The same surface, the same data, the same
//! composition machinery, and every panel redrawn by a different design system.
//!
//! # This is a demonstration, not a deployment pattern
//!
//! Both design systems are compiled into this binary and both are paid for in
//! the bundle a browser downloads. A real front end picks one, as
//! `frontend-demo` and `frontend-aurora` do, and those two differ from each
//! other by a single identifier.
//!
//! What this example is for is showing that the seam is real. `DesignPack`
//! carries an associated view type, which usually costs object safety and here
//! does not, so a pack can be held in a `Box` and chosen at runtime. Nothing in
//! `hlin-ui` changes to allow it and nothing in `hlin-ui` can tell the
//! difference.

use hlin_ui::app::App;
use hlin_view::DesignPack;
use leptos::prelude::*;

/// A pack with its type forgotten, which is what makes a runtime choice
/// possible at all.
type Pack = Box<dyn DesignPack<View = AnyView> + Send + Sync>;

/// What this frontend was built with, in the order it offers them. The first is
/// the default, so a visitor who asks for nothing gets something.
fn packs() -> Vec<(&'static str, Pack)> {
    vec![
        ("demo", Box::new(hlin_pack_demo::DemoPack)),
        ("aurora", Box::new(aurora_leptos::AuroraPack)),
    ]
}

fn main() {
    console_error_panic_hook::set_once();

    let asked = requested();
    let mut available = packs();

    // A name nobody offers falls back to the first rather than failing. The
    // same rule the vocabulary uses for an unknown view kind, and for the same
    // reason: a typo in an address should not produce a blank page.
    let chosen = available
        .iter()
        .position(|(name, _)| Some(*name) == asked.as_deref())
        .unwrap_or(0);

    let (name, pack) = available.remove(chosen);
    leptos::logging::log!("drawing with the `{name}` pack");

    leptos::mount::mount_to_body(move || view! { <App pack=pack /> });
}

/// The pack named in the address, if one is.
fn requested() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;

    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "pack")
        .map(|(_, value)| value.to_string())
}
