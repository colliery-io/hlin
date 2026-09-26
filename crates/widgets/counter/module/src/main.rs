//! The counter's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_counter_components::*;

fn main() {
    hlin_widget_module::mount("counter", Counter);
}
