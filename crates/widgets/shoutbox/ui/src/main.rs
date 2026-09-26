//! The shoutbox's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_shoutbox_components::Shoutbox;

fn main() {
    hlin_widget_ui::direct::mount(Shoutbox);
}
