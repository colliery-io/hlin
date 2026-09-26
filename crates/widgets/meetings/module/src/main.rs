//! The meetings widget's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_meetings_components::*;

fn main() {
    hlin_widget_module::mount("meetings", Meetings);
}
