//! The weather's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_weather_components::Weather;

fn main() {
    hlin_widget_ui::direct::mount(Weather);
}
