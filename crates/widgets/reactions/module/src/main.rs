//! The reactions' Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_reactions_components::*;

fn main() {
    hlin_widget_module::mount("reactions", Reactions);
}
