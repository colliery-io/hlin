//! The sparkline's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_sparkline_components::Sparkline;

fn main() {
    hlin_widget_ui::direct::mount(Sparkline);
}
