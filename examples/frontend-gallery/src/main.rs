//! Hlin's frontend, holding two design systems and choosing between them when
//! the page loads.
//!
//! `?pack=aurora` draws with Colliery's Aurora, and offers its Light / Dark /
//! System switch in the bar. `?pack=demo`, or nothing, draws with the demo
//! pack. The same surface, the same data, the same
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

    // The switch is Aurora's, so only Aurora's page offers it. The demo pack
    // is drawn in one theme, and a switch that changed nothing would lie.
    if name == "aurora" {
        leptos::mount::mount_to_body(move || {
            view! { <App pack=pack bar=|| view! { <aurora_leptos::ThemeToggle /> } /> }
        });
    } else {
        // The theme `index.html` restored is Aurora's choice, and this page
        // is not Aurora's: without this, a person who chose dark there gets
        // dark form controls and scroll bars on the demo pack's light page.
        if let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        {
            let _ = root.remove_attribute("data-theme");
            let _ = root.remove_attribute("style");
        }
        leptos::mount::mount_to_body(move || view! { <App pack=pack /> });
    }
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
