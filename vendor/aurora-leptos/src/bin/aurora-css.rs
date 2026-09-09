//! `aurora-css [dir]` — writes the Aurora Dark stylesheet to `dir/aurora.css`
//! (default `style/`). Leptos-free; run it from a build hook (e.g. a trunk
//! `pre_build` hook) so the app can `<link>` a real, render-blocking stylesheet:
//!
//! ```toml
//! # Trunk.toml
//! [[hooks]]
//! stage = "pre_build"
//! command = "cargo"
//! command_arguments = ["run", "-q", "-p", "aurora-leptos",
//!                      "--no-default-features", "--features", "bin",
//!                      "--bin", "aurora-css", "--", "style"]
//! ```
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "style".to_string());
    let path = aurora_leptos::write_css(std::path::Path::new(&dir)).expect("write aurora.css");
    eprintln!("aurora-css: wrote {}", path.display());
}
