//! The stopwatch's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_stopwatch_components::Stopwatch;

fn main() {
    hlin_widget_ui::direct::mount(Stopwatch);
}
