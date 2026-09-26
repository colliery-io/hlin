//! The on-call rota's own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_oncall_components::Oncall;

fn main() {
    hlin_widget_ui::direct::mount(Oncall);
}
