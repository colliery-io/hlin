//! The dice's Hlin module: its components, mounted with the Hlin client.
//! Nothing else.

pub use hlin_widget_dice_components::*;

fn main() {
    hlin_widget_module::mount("dice", Dice);
}
