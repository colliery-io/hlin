//! Checking an envelope document against what the panel promised.
//!
//! Two things are checked here that parsing alone cannot: that the document is
//! the type the panel's manifest declared, and that it is within the limits the
//! vocabulary sets.
//!
//! Over-limit documents are rejected rather than truncated. Truncation would
//! make a panel lie: a chart drawn from the first five thousand of fifty
//! thousand points looks like a complete chart, and nothing on screen would say
//! otherwise. The `step` hint on `time_range` exists so a platform can be asked
//! to downsample instead (HLIN-S-0002 REQ-3.1).

use std::collections::BTreeSet;
use std::fmt;

use crate::envelope::{
    Envelope, MAX_DOCUMENT_BYTES, OPTIONS_V1, Options, RECORDS_V1, Records, SCALAR_V1, SERIES_V1,
    STATUS_V1, Series, Status,
};

/// The most series one `series.v1` document may carry.
pub const MAX_SERIES: usize = 20;
/// The most points one series may carry.
pub const MAX_POINTS_PER_SERIES: usize = 5_000;
/// The most columns one `records.v1` document may declare.
pub const MAX_COLUMNS: usize = 50;
/// The most rows one `records.v1` document may carry.
pub const MAX_ROWS: usize = 1_000;
/// The most constituent parts one `status.v1` document may list.
pub const MAX_STATUS_ITEMS: usize = 200;
/// The most choices one `options.v1` document may offer.
pub const MAX_OPTIONS: usize = 500;

/// Why an envelope document cannot be used.
///
/// Every variant renders the panel unavailable (malformed). None of them is a
/// shell error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeDefect {
    /// The document was larger than the shell will read.
    TooLarge {
        /// How big it was.
        bytes: usize,
        /// The most it may be.
        limit: usize,
    },
    /// The bytes were not readable as an envelope.
    Unreadable {
        /// What the parser said.
        reason: String,
    },
    /// The document names an envelope this shell does not know.
    UnknownEnvelope {
        /// The name it declared.
        declared: String,
    },
    /// The document is a different type from the one the panel promised.
    Mismatch {
        /// What the manifest said the endpoint returns.
        promised: String,
        /// What the endpoint actually returned.
        returned: String,
    },
    /// A collection was over its limit.
    OverLimit {
        /// Which collection.
        collection: &'static str,
        /// How many there were.
        count: usize,
        /// The most there may be.
        limit: usize,
    },
    /// A collection that must not be empty was.
    Empty {
        /// Which collection.
        collection: &'static str,
    },
    /// A name that must be unique was used twice.
    Duplicate {
        /// What kind of name.
        collection: &'static str,
        /// The repeated name.
        name: String,
    },
    /// A series' points were not in ascending time order, so the shell cannot
    /// tell a gap from a reordering.
    UnorderedPoints {
        /// The series in question.
        series: String,
    },
}

impl fmt::Display for EnvelopeDefect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes, limit } => {
                write!(
                    formatter,
                    "document is {bytes} bytes, over the {limit} byte limit"
                )
            }
            Self::Unreadable { reason } => write!(formatter, "document is not readable: {reason}"),
            Self::UnknownEnvelope { declared } => {
                write!(
                    formatter,
                    "`{declared}` is not an envelope this shell knows"
                )
            }
            Self::Mismatch { promised, returned } => write!(
                formatter,
                "panel promised `{promised}` but the endpoint returned `{returned}`"
            ),
            Self::OverLimit {
                collection,
                count,
                limit,
            } => write!(formatter, "{count} {collection}, over the limit of {limit}"),
            Self::Empty { collection } => write!(formatter, "{collection} is empty"),
            Self::Duplicate { collection, name } => {
                write!(formatter, "{collection} `{name}` appears more than once")
            }
            Self::UnorderedPoints { series } => {
                write!(
                    formatter,
                    "series `{series}` is not in ascending time order"
                )
            }
        }
    }
}

/// Read an envelope document and check it against what the panel promised.
///
/// `promised` is the envelope name from the panel's manifest declaration. The
/// document's own `envelope` field must agree with it.
pub fn parse_envelope(bytes: &[u8], promised: &str) -> Result<Envelope, EnvelopeDefect> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(EnvelopeDefect::TooLarge {
            bytes: bytes.len(),
            limit: MAX_DOCUMENT_BYTES,
        });
    }

    if !crate::envelope::is_known(promised) {
        return Err(EnvelopeDefect::UnknownEnvelope {
            declared: promised.to_string(),
        });
    }

    let declared = declared_name(bytes)?;
    if declared != promised {
        return Err(EnvelopeDefect::Mismatch {
            promised: promised.to_string(),
            returned: declared,
        });
    }

    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|error| EnvelopeDefect::Unreadable {
            reason: error.to_string(),
        })?;

    validate(&envelope)?;
    Ok(envelope)
}

/// The envelope name a document declares for itself, before it is parsed as one.
///
/// Read separately so that a document of the wrong type is reported as a
/// mismatch rather than as unreadable: "you promised a series and sent a table"
/// is a far more useful thing to tell an operator than a deserialisation error
/// about an unknown variant.
fn declared_name(bytes: &[u8]) -> Result<String, EnvelopeDefect> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| EnvelopeDefect::Unreadable {
            reason: error.to_string(),
        })?;

    match document.get("envelope") {
        Some(serde_json::Value::String(name)) if crate::envelope::is_known(name) => {
            Ok(name.clone())
        }
        Some(serde_json::Value::String(name)) => Err(EnvelopeDefect::UnknownEnvelope {
            declared: name.clone(),
        }),
        Some(_) => Err(EnvelopeDefect::Unreadable {
            reason: "`envelope` is not a string".to_string(),
        }),
        None => Err(EnvelopeDefect::Unreadable {
            reason: "document does not declare an `envelope`".to_string(),
        }),
    }
}

/// Check a parsed envelope against the vocabulary's limits.
pub fn validate(envelope: &Envelope) -> Result<(), EnvelopeDefect> {
    match envelope {
        Envelope::Scalar(_) => Ok(()),
        Envelope::Series(series) => validate_series(series),
        Envelope::Records(records) => validate_records(records),
        Envelope::Status(status) => validate_status(status),
        Envelope::Options(options) => validate_options(options),
    }
}

fn validate_series(document: &Series) -> Result<(), EnvelopeDefect> {
    if document.series.is_empty() {
        return Err(EnvelopeDefect::Empty {
            collection: "series",
        });
    }
    at_most("series", document.series.len(), MAX_SERIES)?;

    let mut names = BTreeSet::new();
    for line in &document.series {
        if !names.insert(line.name.as_str()) {
            return Err(EnvelopeDefect::Duplicate {
                collection: "series",
                name: line.name.clone(),
            });
        }
        at_most("points", line.points.len(), MAX_POINTS_PER_SERIES)?;

        let ascending = line
            .points
            .windows(2)
            .all(|pair| pair[0].at() < pair[1].at());
        if !ascending {
            return Err(EnvelopeDefect::UnorderedPoints {
                series: line.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_records(document: &Records) -> Result<(), EnvelopeDefect> {
    if document.columns.is_empty() {
        return Err(EnvelopeDefect::Empty {
            collection: "columns",
        });
    }
    at_most("columns", document.columns.len(), MAX_COLUMNS)?;
    at_most("rows", document.rows.len(), MAX_ROWS)?;

    let mut keys = BTreeSet::new();
    for column in &document.columns {
        if !keys.insert(column.key.as_str()) {
            return Err(EnvelopeDefect::Duplicate {
                collection: "column",
                name: column.key.clone(),
            });
        }
    }
    Ok(())
}

fn validate_status(document: &Status) -> Result<(), EnvelopeDefect> {
    at_most("status items", document.items.len(), MAX_STATUS_ITEMS)?;

    let mut names = BTreeSet::new();
    for item in &document.items {
        if !names.insert(item.name.as_str()) {
            return Err(EnvelopeDefect::Duplicate {
                collection: "status item",
                name: item.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_options(document: &Options) -> Result<(), EnvelopeDefect> {
    at_most("options", document.options.len(), MAX_OPTIONS)?;

    let mut values = BTreeSet::new();
    for choice in &document.options {
        if !values.insert(choice.value.as_str()) {
            return Err(EnvelopeDefect::Duplicate {
                collection: "option",
                name: choice.value.clone(),
            });
        }
    }
    Ok(())
}

fn at_most(collection: &'static str, count: usize, limit: usize) -> Result<(), EnvelopeDefect> {
    if count > limit {
        Err(EnvelopeDefect::OverLimit {
            collection,
            count,
            limit,
        })
    } else {
        Ok(())
    }
}

/// The vocabulary names, for callers that want to report what is available.
pub fn known_envelopes() -> [&'static str; 5] {
    [SCALAR_V1, SERIES_V1, RECORDS_V1, STATUS_V1, OPTIONS_V1]
}
