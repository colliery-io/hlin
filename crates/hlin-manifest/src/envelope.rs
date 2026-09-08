//! The typed documents a panel's data endpoint returns.
//!
//! An envelope is pure data for a view kind: values, units, labels and
//! timestamps, and nothing about how any of it looks. Colours, thresholds and
//! sizes are the shell's business, and a panel's *state* travels alongside the
//! envelope in the stream rather than inside it (decision HLIN-A-0001), so
//! there is nothing here for a platform to say about presentation even by
//! accident.
//!
//! Every document names its own type in an `envelope` field. The shell checks
//! that self-declaration against what the panel promised, so a platform that
//! changes what an endpoint returns without changing its manifest is caught by
//! the data rather than trusted on its word (HLIN-S-0002 REQ-1.1).
//!
//! Envelope versions are permanent. Platforms can never be forced to upgrade,
//! so once anyone names `series.v1` the shell accepts it forever; a version
//! suffix means a new shape exists alongside the old one, never that the old
//! one is going away (decision HLIN-A-0003).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The largest envelope document the shell will read, in bytes.
///
/// Applies to every envelope regardless of its own limits.
pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

/// Units the design system knows how to format.
///
/// A platform may use any other string, which is shown verbatim as a suffix;
/// these are simply the ones that get real formatting.
pub const FORMATTED_UNITS: [&str; 7] = [
    "count",
    "percent",
    "bytes",
    "bytes_per_second",
    "seconds",
    "milliseconds",
    "per_second",
];

/// A document returned by a panel's data endpoint.
///
/// The variants are the v1 vocabulary. Adding one is an additive change to this
/// enum and a release of this crate; removing one is not a supported operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "envelope")]
pub enum Envelope {
    /// One value.
    #[serde(rename = "scalar.v1")]
    Scalar(Scalar),
    /// One or more time series.
    #[serde(rename = "series.v1")]
    Series(Series),
    /// Tabular data.
    #[serde(rename = "records.v1")]
    Records(Records),
    /// The health of one thing, or of a list of things.
    #[serde(rename = "status.v1")]
    Status(Status),
    /// The choices a `select` parameter offers.
    #[serde(rename = "options.v1")]
    Options(Options),
}

impl Envelope {
    /// The vocabulary name this document declares itself to be.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Scalar(_) => SCALAR_V1,
            Self::Series(_) => SERIES_V1,
            Self::Records(_) => RECORDS_V1,
            Self::Status(_) => STATUS_V1,
            Self::Options(_) => OPTIONS_V1,
        }
    }

    /// When the data was true, where the platform said so.
    ///
    /// The aggregator prefers this to fetch time when deciding whether a panel
    /// has gone stale, so that a platform serving cached data can be honest
    /// about it.
    pub fn as_of(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Scalar(inner) => inner.as_of,
            Self::Series(inner) => inner.as_of,
            Self::Records(inner) => inner.as_of,
            Self::Status(inner) => inner.as_of,
            Self::Options(inner) => inner.as_of,
        }
    }
}

/// The `scalar.v1` vocabulary name.
pub const SCALAR_V1: &str = "scalar.v1";
/// The `series.v1` vocabulary name.
pub const SERIES_V1: &str = "series.v1";
/// The `records.v1` vocabulary name.
pub const RECORDS_V1: &str = "records.v1";
/// The `status.v1` vocabulary name.
pub const STATUS_V1: &str = "status.v1";
/// The `options.v1` vocabulary name.
pub const OPTIONS_V1: &str = "options.v1";

/// Every envelope name in the v1 vocabulary.
pub const VOCABULARY: [&str; 5] = [SCALAR_V1, SERIES_V1, RECORDS_V1, STATUS_V1, OPTIONS_V1];

/// Whether a name is in the envelope vocabulary.
pub fn is_known(envelope: &str) -> bool {
    VOCABULARY.contains(&envelope)
}

/// One value, with enough context to render it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scalar {
    /// The value itself. `null` where the platform has nothing to report.
    pub value: Value,

    /// Unit for formatting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,

    /// Short caption under the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// A comparison value. The kind decides how to show the delta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<f64>,

    /// When this was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One or more time series over the same window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// The series. At least one.
    pub series: Vec<Line>,

    /// Unit, applying to every series in the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,

    /// When this was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One named series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// Name, unique within the document. Becomes the legend entry.
    pub name: String,

    /// Points in ascending time order. A `null` value is a gap rather than a
    /// zero, and is drawn as a break in the line.
    pub points: Vec<Point>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One sample: a timestamp in epoch milliseconds, and a value that may be
/// absent.
///
/// Points are the one place the contract uses epoch milliseconds rather than
/// RFC 3339, because they are the one place a document holds thousands of
/// timestamps and the size difference is the difference between a panel that
/// loads and one that does not.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point(pub i64, pub Option<f64>);

impl Point {
    /// The sample's timestamp, in epoch milliseconds.
    pub fn at(&self) -> i64 {
        self.0
    }

    /// The sample's value, absent where there is a gap.
    pub fn value(&self) -> Option<f64> {
        self.1
    }
}

/// Tabular data: typed columns, and rows keyed by column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Records {
    /// Columns in display order. At least one.
    pub columns: Vec<Column>,

    /// Rows, each an object keyed by column key. A missing key renders empty;
    /// a key no column declares is ignored.
    pub rows: Vec<serde_json::Map<String, Value>>,

    /// When this was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One column of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    /// The key rows use for this column. Unique within the document.
    pub key: String,

    /// Display label.
    pub label: String,

    /// What kind of value this column holds. Drives formatting and sorting.
    #[serde(rename = "type")]
    pub value_type: ColumnType,

    /// Unit, for numeric columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    /// Text.
    String,
    /// A number, formatted by the column's unit.
    Number,
    /// True or false.
    Boolean,
    /// An instant, given as RFC 3339.
    Timestamp,
    /// A duration in milliseconds.
    DurationMs,
}

/// The health of one thing, and optionally of its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The rollup.
    pub status: Health,

    /// What this is the status of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// One line of detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// When the current status began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,

    /// The constituent parts, where there are any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<StatusItem>,

    /// When this was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One constituent part of a status rollup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusItem {
    /// What this part is called.
    pub name: String,

    /// Its health.
    pub status: Health,

    /// One line of detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// How healthy something is.
///
/// Deliberately four states rather than a number: a platform reports what it
/// knows, and `unknown` is a real answer rather than a failure to give one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    /// Working.
    Ok,
    /// Working, but not properly.
    Degraded,
    /// Not working.
    Down,
    /// The platform cannot tell.
    Unknown,
}

/// The choices a `select` parameter offers.
///
/// Fetched with the viewer's forwarded identity, so two people may legitimately
/// see different options for the same panel (decision HLIN-A-0004).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// The choices. May be empty, which the kind renders as "no options"
    /// rather than as a failure.
    pub options: Vec<Choice>,

    /// When this was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    /// What is sent back on the data request. Unique within the document.
    pub value: String,

    /// What the person picking sees.
    pub label: String,

    /// Visual grouping only; carries no meaning to the platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// What an envelope is for.
///
/// Most envelopes are what a panel's data endpoint returns, and a view kind
/// draws them. `options.v1` is not: it is what a `select` parameter's options
/// endpoint returns, and the shell renders it as a control rather than as a
/// panel. The distinction matters in two places. A panel may not declare a
/// parameter envelope, because no kind draws one meaningfully. And the
/// governance rule that every envelope needs an accepting kind besides `raw`
/// applies only to the ones a panel can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Returned by a panel's data endpoint and drawn by a view kind.
    Panel,
    /// Returned by a parameter's endpoint and rendered as a control.
    Parameter,
}

/// What an envelope is for, by name.
///
/// An unknown name has no role, which is a separate question from whether a
/// panel may declare it.
pub fn role(envelope: &str) -> Option<Role> {
    match envelope {
        SCALAR_V1 | SERIES_V1 | RECORDS_V1 | STATUS_V1 => Some(Role::Panel),
        OPTIONS_V1 => Some(Role::Parameter),
        _ => None,
    }
}

/// The envelopes a panel may declare.
pub const PANEL_VOCABULARY: [&str; 4] = [SCALAR_V1, SERIES_V1, RECORDS_V1, STATUS_V1];

/// Whether a panel may declare this envelope for its data endpoint.
pub fn is_panel_envelope(envelope: &str) -> bool {
    role(envelope) == Some(Role::Panel)
}
