//! The shoutbox's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_shoutbox_components::*;

fn main() {
    hlin_widget_module::mount("shoutbox", Shoutbox);
}
