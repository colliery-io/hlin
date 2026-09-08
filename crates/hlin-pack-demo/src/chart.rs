//! Drawing a line chart, by hand.
//!
//! Deliberately no charting dependency. A real design system will bring its
//! own, and inheriting one here would put it in the shell's dependency graph
//! for the life of the project to draw six demo panels. Linear scales and a
//! path string are not much code, and being small is this pack's whole point.

use hlin_manifest::envelope::{Line, Series};

/// The drawing area, in the chart's own coordinate space.
pub const WIDTH: f64 = 600.0;
/// The drawing area's height.
pub const HEIGHT: f64 = 200.0;
/// Room for the axis labels.
pub const PADDING: f64 = 28.0;

/// The colours a series takes, in order.
///
/// Six is enough for the demo and the envelope allows twenty; beyond six they
/// repeat, which is honest for a pack that says it is minimal.
pub const COLOURS: [&str; 6] = [
    "#4c6ef5", "#e8590c", "#2f9e44", "#ae3ec9", "#1098ad", "#e03131",
];

/// The extent of the data, for scaling.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// Earliest instant, epoch milliseconds.
    pub from: f64,
    /// Latest instant.
    pub to: f64,
    /// Smallest value, never above zero so a line does not float.
    pub low: f64,
    /// Largest value.
    pub high: f64,
}

impl Bounds {
    /// The extent of every point in a document.
    ///
    /// Returns `None` for a document with nothing to draw, which the caller
    /// renders as an empty chart rather than dividing by zero.
    pub fn of(series: &Series) -> Option<Self> {
        let points: Vec<(i64, f64)> = series
            .series
            .iter()
            .flat_map(|line| line.points.iter())
            .filter_map(|point| point.value().map(|value| (point.at(), value)))
            .collect();

        if points.is_empty() {
            return None;
        }

        let from = points.iter().map(|(at, _)| *at).min().unwrap() as f64;
        let to = points.iter().map(|(at, _)| *at).max().unwrap() as f64;
        let low = points.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
        let high = points
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::NEG_INFINITY, f64::max);

        Some(Self {
            from,
            to: if to > from { to } else { from + 1.0 },
            low: low.min(0.0),
            high: if high > low { high } else { low + 1.0 },
        })
    }

    fn x(&self, at: i64) -> f64 {
        let span = self.to - self.from;
        PADDING + (at as f64 - self.from) / span * (WIDTH - PADDING * 2.0)
    }

    fn y(&self, value: f64) -> f64 {
        let span = self.high - self.low;
        HEIGHT - PADDING - (value - self.low) / span * (HEIGHT - PADDING * 2.0)
    }
}

/// An SVG path for one series.
///
/// A `null` starts a new subpath rather than joining across it, so a gap in
/// the data is drawn as a gap. Joining would show a straight line through
/// missing time, which is the difference between "we do not know" and "it was
/// fine", and is the classic way a chart lies.
pub fn path(line: &Line, bounds: Bounds) -> String {
    let mut path = String::new();
    let mut pen_down = false;

    for point in &line.points {
        match point.value() {
            Some(value) => {
                let x = bounds.x(point.at());
                let y = bounds.y(value);
                if pen_down {
                    path.push_str(&format!(" L{x:.1} {y:.1}"));
                } else {
                    path.push_str(&format!(" M{x:.1} {y:.1}"));
                    pen_down = true;
                }
            }
            None => pen_down = false,
        }
    }

    path.trim().to_string()
}

/// A short line for a sparkline, scaled to its own box.
pub fn spark_path(line: &Line, width: f64, height: f64) -> String {
    let values: Vec<(i64, f64)> = line
        .points
        .iter()
        .filter_map(|point| point.value().map(|value| (point.at(), value)))
        .collect();

    if values.len() < 2 {
        return String::new();
    }

    let first = values.first().unwrap().0 as f64;
    let last = values.last().unwrap().0 as f64;
    let span = (last - first).max(1.0);
    let low = values.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let high = values
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (high - low).max(f64::EPSILON);

    let mut path = String::new();
    let mut pen_down = false;
    for point in &line.points {
        match point.value() {
            Some(value) => {
                let x = (point.at() as f64 - first) / span * width;
                let y = height - (value - low) / range * height;
                if pen_down {
                    path.push_str(&format!(" L{x:.1} {y:.1}"));
                } else {
                    path.push_str(&format!(" M{x:.1} {y:.1}"));
                    pen_down = true;
                }
            }
            None => pen_down = false,
        }
    }
    path.trim().to_string()
}

/// The latest value a series reported, for a sparkline's caption.
pub fn latest(line: &Line) -> Option<f64> {
    line.points.iter().rev().find_map(|point| point.value())
}

/// The colour for the series at this position.
pub fn colour(index: usize) -> &'static str {
    COLOURS[index % COLOURS.len()]
}
