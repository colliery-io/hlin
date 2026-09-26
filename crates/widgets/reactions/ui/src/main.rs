//! The reactions' own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_reactions_components::Reactions;

fn main() {
    hlin_widget_ui::direct::mount(Reactions);
}
