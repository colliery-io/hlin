//! The picker's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_picker_components::Picker;

fn main() {
    hlin_widget_ui::direct::mount(Picker);
}
