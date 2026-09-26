//! The on-call rota's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_oncall_components::*;

fn main() {
    hlin_widget_module::mount("oncall", Oncall);
}
