//! The poll's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_poll_components::Poll;

fn main() {
    hlin_widget_ui::direct::mount(Poll);
}
