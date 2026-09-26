//! The weather's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_weather_components::*;

fn main() {
    hlin_widget_module::mount("weather", Weather);
}
