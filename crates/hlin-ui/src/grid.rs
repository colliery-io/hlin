//! Where panels sit, and what happens when one is moved onto another.
//!
//! No DOM, no Leptos, no pixels except where a gesture is converted into
//! cells. Everything here is a function from placements to placements, which
//! is what makes the one property worth being sure about testable: after any
//! sequence of operations, no two panels overlap.
//!
//! The grid has gravity. A panel floats up until something stops it, and a
//! panel dropped onto another pushes it down. That is one rule rather than
//! two, and it means there is no such thing as a layout with a hole above a
//! panel, which in turn means a surface never renders as a scattering of
//! panels with empty bands between them.
//!
//! This is written to be replaced. A real design pack will one day want to own
//! the grid, so the interface is deliberately small: a list of placements in,
//! the same list out.

use hlin_stream::layout::{COLUMNS, Placement};

/// The default size of a panel nobody has resized.
pub const DEFAULT_SIZE: (u32, u32) = (4, 4);

/// The first placement of this size that overlaps nothing.
///
/// Scanned row by row, left to right, so adding panels one after another fills
/// the surface the way reading does. A surface with no room in any existing
/// row gets a new one below everything, so this always answers.
pub fn first_free(taken: &[Placement], w: u32, h: u32) -> Placement {
    let w = w.clamp(1, COLUMNS);
    let h = h.max(1);
    let floor = taken.iter().map(Placement::bottom).max().unwrap_or(0);

    for y in 0..=floor {
        for x in 0..=(COLUMNS - w) {
            let candidate = Placement { x, y, w, h };
            if !taken.iter().any(|placed| placed.overlaps(&candidate)) {
                return candidate;
            }
        }
    }

    // Nothing fits beside what is already there, so it goes underneath.
    Placement {
        x: 0,
        y: floor,
        w,
        h,
    }
}

/// Resolve a set of placements into an arrangement with no overlaps.
///
/// `priority`, where given, is the index of the panel the viewer just moved or
/// resized. It is placed before anything it ties with, so a panel dropped onto
/// another takes the spot and the other gives way, rather than the other way
/// around.
///
/// The rule per panel, in order: clamp it into the grid, push it down until it
/// clears everything already placed, then float it back up as far as it can
/// go. Push-then-float rather than float-then-push, because a panel that
/// currently overlaps has no meaningful "up" to try first.
pub fn settle(placements: &mut [Placement], priority: Option<usize>) {
    let mut order: Vec<usize> = (0..placements.len()).collect();
    order.sort_by_key(|&index| {
        (
            placements[index].y,
            placements[index].x,
            // Among panels that would otherwise tie, the moved one goes first.
            u8::from(priority != Some(index)),
        )
    });

    let mut settled: Vec<Placement> = Vec::with_capacity(placements.len());

    for index in order {
        let mut candidate = placements[index].clamped();

        while settled.iter().any(|placed| placed.overlaps(&candidate)) {
            candidate.y += 1;
        }

        while candidate.y > 0 {
            let higher = Placement {
                y: candidate.y - 1,
                ..candidate
            };
            if settled.iter().any(|placed| placed.overlaps(&higher)) {
                break;
            }
            candidate = higher;
        }

        placements[index] = candidate;
        settled.push(candidate);
    }
}

/// Move one panel to a column and row, and settle everything around it.
pub fn move_to(placements: &mut [Placement], index: usize, x: u32, y: u32) {
    if index >= placements.len() {
        return;
    }
    placements[index].x = x.min(COLUMNS.saturating_sub(placements[index].w));
    placements[index].y = y;
    settle(placements, Some(index));
}

/// Resize one panel, and settle everything around it.
///
/// Width is clamped to the grid from the panel's own left edge, so dragging a
/// corner past the right-hand side widens the panel to the edge and stops,
/// rather than sliding the panel leftwards under the pointer.
pub fn resize(placements: &mut [Placement], index: usize, w: u32, h: u32) {
    if index >= placements.len() {
        return;
    }
    let x = placements[index].x;
    placements[index].w = w.clamp(1, COLUMNS - x);
    placements[index].h = h.max(1);
    settle(placements, Some(index));
}

/// How many whole cells a pointer has travelled.
///
/// Rounded rather than truncated, so a drag of most of a cell lands in the next
/// one and the panel follows the pointer instead of lagging behind it. A cell
/// width of zero, which is what a browser reports for a grid it has not laid
/// out yet, means no movement rather than a division by zero.
pub fn cells(delta_pixels: f64, cell_pixels: f64) -> i32 {
    if cell_pixels <= 0.0 {
        return 0;
    }
    (delta_pixels / cell_pixels).round() as i32
}

/// A column or row shifted by a gesture, never below zero.
pub fn shifted(origin: u32, delta: i32) -> u32 {
    (origin as i64 + delta as i64).max(0) as u32
}

/// How many rows the arrangement occupies.
pub fn depth(placements: &[Placement]) -> u32 {
    placements.iter().map(Placement::bottom).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: u32, y: u32, w: u32, h: u32) -> Placement {
        Placement { x, y, w, h }
    }

    /// The property the whole module exists to hold.
    fn no_overlaps(placements: &[Placement]) {
        for (index, one) in placements.iter().enumerate() {
            for other in &placements[index + 1..] {
                assert!(
                    !one.overlaps(other),
                    "{one:?} and {other:?} cover the same cells"
                );
            }
        }
    }

    #[test]
    fn panels_fill_the_first_row_before_starting_a_second() {
        let mut taken: Vec<Placement> = Vec::new();
        for _ in 0..3 {
            let next = first_free(&taken, 4, 4);
            taken.push(next);
        }

        assert_eq!(taken[0], at(0, 0, 4, 4));
        assert_eq!(taken[1], at(4, 0, 4, 4));
        assert_eq!(taken[2], at(8, 0, 4, 4));

        // The row is full at twelve columns, so the fourth starts a new one.
        let fourth = first_free(&taken, 4, 4);
        assert_eq!(fourth, at(0, 4, 4, 4));
    }

    #[test]
    fn a_gap_left_by_a_removal_is_filled_before_the_bottom() {
        let taken = vec![at(0, 0, 4, 4), at(8, 0, 4, 4)];
        assert_eq!(
            first_free(&taken, 4, 4),
            at(4, 0, 4, 4),
            "the hole in the middle is the first free place, not the next row"
        );
    }

    #[test]
    fn a_panel_wider_than_the_gap_goes_below_it() {
        let taken = vec![at(0, 0, 4, 4), at(8, 0, 4, 4)];
        let wide = first_free(&taken, 8, 2);
        assert_eq!(wide.y, 4, "eight columns do not fit in a gap of four");
    }

    #[test]
    fn dropping_a_panel_onto_another_pushes_that_one_down() {
        // Two panels sharing a column, the second dropped onto the first.
        let mut placements = vec![at(0, 0, 6, 4), at(0, 0, 6, 4)];
        settle(&mut placements, Some(1));

        assert_eq!(
            placements[1],
            at(0, 0, 6, 4),
            "the moved panel takes the spot"
        );
        assert_eq!(placements[0], at(0, 4, 6, 4), "and the other gives way");
        no_overlaps(&placements);
    }

    #[test]
    fn a_panel_floats_up_into_the_space_above_it() {
        let mut placements = vec![at(0, 9, 4, 4), at(6, 3, 4, 2)];
        settle(&mut placements, None);

        assert_eq!(
            placements[0].y, 0,
            "nothing above it, so it rises to the top"
        );
        assert_eq!(
            placements[1].y, 0,
            "and so does the other, in its own columns"
        );
        no_overlaps(&placements);
    }

    #[test]
    fn floating_up_stops_at_what_is_above() {
        let mut placements = vec![at(0, 0, 6, 3), at(0, 8, 6, 3)];
        settle(&mut placements, None);

        assert_eq!(placements[0].y, 0);
        assert_eq!(
            placements[1].y, 3,
            "it rises until it touches the panel above, and no further"
        );
        no_overlaps(&placements);
    }

    #[test]
    fn a_move_settles_everything_it_displaces_in_turn() {
        // Three stacked panels; the bottom one is dragged to the top.
        let mut placements = vec![at(0, 0, 12, 2), at(0, 2, 12, 2), at(0, 4, 12, 2)];
        move_to(&mut placements, 2, 0, 0);

        assert_eq!(
            placements[2].y, 0,
            "the dragged panel lands where it was put"
        );
        no_overlaps(&placements);
        assert_eq!(
            depth(&placements),
            6,
            "and the cascade leaves no gap: three panels of two rows fill six"
        );
    }

    #[test]
    fn resizing_is_clamped_to_the_grid_from_the_panels_own_edge() {
        let mut placements = vec![at(8, 0, 4, 4)];
        resize(&mut placements, 0, 9, 3);

        assert_eq!(
            placements[0].w, 4,
            "it grows to the right-hand edge and stops, rather than sliding left"
        );
        assert_eq!(placements[0].x, 8, "so its left edge does not move");
        assert_eq!(placements[0].h, 3);

        resize(&mut placements, 0, 0, 0);
        assert_eq!(
            (placements[0].w, placements[0].h),
            (1, 1),
            "and nothing can be resized out of existence"
        );
    }

    #[test]
    fn widening_a_panel_pushes_what_it_grows_into_out_of_the_way() {
        let mut placements = vec![at(0, 0, 4, 4), at(4, 0, 4, 4)];
        resize(&mut placements, 0, 8, 4);

        assert_eq!(placements[0], at(0, 0, 8, 4));
        assert_eq!(placements[1].y, 4, "its neighbour moves down, not sideways");
        no_overlaps(&placements);
    }

    #[test]
    fn no_sequence_of_operations_leaves_two_panels_overlapping() {
        // A deterministic walk through moves and resizes, checking the
        // invariant after every one. Not random, so a failure is reproducible.
        let mut placements: Vec<Placement> =
            (0..6).map(|n| at((n % 3) * 4, (n / 3) * 3, 4, 3)).collect();

        let script: [(usize, u32, u32, bool); 10] = [
            (0, 4, 0, false),
            (5, 0, 0, false),
            (2, 8, 12, false),
            (1, 6, 5, true),
            (3, 0, 9, false),
            (4, 11, 1, true),
            (0, 12, 2, true),
            (2, 0, 0, false),
            (5, 3, 7, false),
            (1, 1, 1, true),
        ];

        for (index, a, b, is_resize) in script {
            if is_resize {
                resize(&mut placements, index, a, b);
            } else {
                move_to(&mut placements, index, a, b);
            }

            no_overlaps(&placements);
            for placement in &placements {
                assert!(
                    placement.right() <= COLUMNS,
                    "{placement:?} runs off the grid"
                );
                assert!(placement.w >= 1 && placement.h >= 1);
            }
        }

        assert_eq!(placements.len(), 6, "nothing was lost along the way");
    }

    #[test]
    fn a_gesture_becomes_whole_cells() {
        assert_eq!(cells(0.0, 80.0), 0);
        assert_eq!(cells(41.0, 80.0), 1, "past halfway is the next cell");
        assert_eq!(cells(39.0, 80.0), 0, "and short of it is not");
        assert_eq!(cells(-120.0, 80.0), -2);
        assert_eq!(
            cells(500.0, 0.0),
            0,
            "a grid the browser has not measured yet moves nothing"
        );
    }

    #[test]
    fn a_gesture_cannot_drag_a_panel_above_the_first_row() {
        assert_eq!(shifted(2, -5), 0);
        assert_eq!(shifted(2, 3), 5);
    }
}
