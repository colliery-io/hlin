//! The view-kind vocabulary and what each kind will draw.
//!
//! Kinds are a bounded set, owned here and grown deliberately. A platform
//! adopts one by naming it in a manifest; it cannot introduce one, because a
//! name the shell does not know falls back to [`Kind::Raw`] rather than
//! becoming a new kind by assertion.
//!
//! The acceptance matrix is the seam between the two vocabularies. Kinds change
//! when the design system decides something should *look* different; envelopes
//! change when platforms need to *say* something different. Keeping them apart
//! is what lets each move on its own cadence (decision HLIN-A-0003), and this
//! table is where they meet.

use std::fmt;

use hlin_manifest::envelope::{OPTIONS_V1, RECORDS_V1, SCALAR_V1, SERIES_V1, STATUS_V1};

/// A way of drawing a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// One value, formatted by its unit, with its label and any delta.
    Stat,
    /// A line chart over time, with a legend and visible gaps.
    Timeseries,
    /// A compact line and its latest value, with no axes.
    Sparkline,
    /// A sortable table.
    Table,
    /// A health rollup, with its parts when there are any.
    Status,
    /// The panel's title, its envelope name, and the document as a key/value
    /// tree.
    ///
    /// `Raw` is what makes rendering total. It accepts every envelope, it is
    /// where an unknown kind name lands, and it is therefore the guarantee that
    /// no panel a platform can declare is undrawable. It is deliberately plain:
    /// useful enough to read, unattractive enough that nobody ships it on
    /// purpose.
    Raw,
}

/// Every kind in the vocabulary.
pub const VOCABULARY: [Kind; 6] = [
    Kind::Stat,
    Kind::Timeseries,
    Kind::Sparkline,
    Kind::Table,
    Kind::Status,
    Kind::Raw,
];

impl Kind {
    /// The vocabulary name a manifest uses for this kind.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stat => "stat",
            Self::Timeseries => "timeseries",
            Self::Sparkline => "sparkline",
            Self::Table => "table",
            Self::Status => "status",
            Self::Raw => "raw",
        }
    }

    /// Resolve a name from a manifest.
    ///
    /// An unknown name is [`Kind::Raw`] rather than an error: a platform naming
    /// a kind this shell has not learned yet still gets a panel, and the layout
    /// around it is undisturbed (HLIN-S-0002 REQ-1.4).
    pub fn resolve(name: &str) -> Self {
        VOCABULARY
            .into_iter()
            .find(|kind| kind.name() == name)
            .unwrap_or(Self::Raw)
    }

    /// Whether this shell knows the name, as opposed to falling back.
    pub fn is_known(name: &str) -> bool {
        VOCABULARY.iter().any(|kind| kind.name() == name)
    }

    /// The envelopes this kind can draw.
    pub fn accepts(&self) -> &'static [&'static str] {
        match self {
            Self::Stat => &[SCALAR_V1],
            Self::Timeseries => &[SERIES_V1],
            Self::Sparkline => &[SERIES_V1],
            // A table can lay out a series as one row per timestamp and one
            // column per series, which gives every chart a readable form for
            // free.
            Self::Table => &[RECORDS_V1, SERIES_V1],
            Self::Status => &[STATUS_V1],
            Self::Raw => &[SCALAR_V1, SERIES_V1, RECORDS_V1, STATUS_V1, OPTIONS_V1],
        }
    }

    /// Whether this kind can draw that envelope.
    pub fn accepts_envelope(&self, envelope: &str) -> bool {
        self.accepts().contains(&envelope)
    }

    /// Every kind that can draw this envelope.
    ///
    /// This is the list a person is offered when switching a panel's rendering,
    /// which is a layout choice needing no deploy from anyone.
    pub fn accepting(envelope: &str) -> Vec<Kind> {
        VOCABULARY
            .into_iter()
            .filter(|kind| kind.accepts_envelope(envelope))
            .collect()
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Whether a kind may draw an envelope, by name.
///
/// The shell checks this when it ingests a manifest. A pairing the matrix does
/// not allow makes that panel unavailable (malformed) and leaves the rest of
/// the platform alone (decision HLIN-A-0003).
pub fn accepts(kind: &str, envelope: &str) -> bool {
    Kind::resolve(kind).accepts_envelope(envelope)
}
