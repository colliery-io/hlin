//! The bookmarks' own UI, at the root of its origin: its components, mounted
//! with the direct client. Nothing else.

use hlin_widget_bookmarks_components::Bookmarks;

fn main() {
    hlin_widget_ui::direct::mount(Bookmarks);
}
