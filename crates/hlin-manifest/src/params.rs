//! The shared parameter vocabulary.
//!
//! A panel declares the shell-level controls it responds to by naming them.
//! The manifest never carries a parameter's type or its rendering: a parameter
//! entry is a vocabulary name plus configuration that is itself data
//! (specification HLIN-S-0001), and this module is the vocabulary.
//!
//! Parameters live here rather than in `hlin-view` because they are contract
//! rather than rendering. What a parameter is called, what configuration it
//! takes, and how its value is encoded onto a data request are all things a
//! platform must agree with the shell about; how a time picker *looks* is not.
//!
//! The vocabulary is deliberately small. Growing it is a shell release, and
//! admission requires a configuration schema, a query encoding that does not
//! collide with the reserved names, and a statement of whether the parameter's
//! value is layout state or global shell state (HLIN-S-0002).

use serde_json::{Map, Value};

/// Query parameter names the shell reserves for [`TIME_RANGE`].
///
/// A `select` may not claim one of these as its `id`, or the two would collide
/// on the same data request.
pub const RESERVED_QUERY_NAMES: [&str; 3] = ["from", "to", "step"];

/// The shell's global time picker. Takes no configuration.
///
/// Encoded as `from=<RFC 3339>&to=<RFC 3339>&step=<integer seconds>`. The
/// shell resolves relative expressions to absolute instants before encoding, so
/// a platform never sees `now-1h` (HLIN-S-0002 REQ-2.2).
pub const TIME_RANGE: &str = "time_range";

/// A panel-scoped chooser whose options come from the platform.
///
/// This is how a panel is parameterised by cluster, tenant, region, or any
/// other entity without minting a panel key per entity. Configuration: `id`
/// (the query name), `label`, `options` (a path returning an `options.v1`
/// envelope) and optional `multiple`.
pub const SELECT: &str = "select";

/// Every parameter name in the vocabulary.
pub const VOCABULARY: [&str; 2] = [TIME_RANGE, SELECT];

/// Whether a name is in the vocabulary.
pub fn is_known(param: &str) -> bool {
    VOCABULARY.contains(&param)
}

/// Why a parameter declaration cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamDefect {
    /// The name is not in the vocabulary. Growing the vocabulary is a shell
    /// release; a platform cannot introduce a parameter by declaring one.
    Unknown {
        /// The name the panel declared.
        param: String,
    },
    /// A configuration field the vocabulary entry requires was absent.
    MissingConfig {
        /// The parameter that requires it.
        param: String,
        /// The field that was missing.
        field: String,
    },
    /// A configuration field was present but the wrong shape.
    InvalidConfig {
        /// The parameter it belongs to.
        param: String,
        /// The offending field.
        field: String,
        /// What was expected instead.
        expected: String,
    },
    /// A `select` claimed a query name the shell reserves for `time_range`.
    ReservedQueryName {
        /// The name it tried to claim.
        id: String,
    },
    /// The parameter takes no configuration, but configuration was supplied.
    UnexpectedConfig {
        /// The parameter that takes none.
        param: String,
    },
}

/// Check one parameter declaration against the vocabulary.
pub fn validate(param: &str, config: &Map<String, Value>) -> Result<(), ParamDefect> {
    match param {
        TIME_RANGE => {
            if config.is_empty() {
                Ok(())
            } else {
                Err(ParamDefect::UnexpectedConfig {
                    param: param.to_string(),
                })
            }
        }
        SELECT => validate_select(config),
        _ => Err(ParamDefect::Unknown {
            param: param.to_string(),
        }),
    }
}

fn validate_select(config: &Map<String, Value>) -> Result<(), ParamDefect> {
    let id = required_string(SELECT, config, "id")?;
    if RESERVED_QUERY_NAMES.contains(&id) {
        return Err(ParamDefect::ReservedQueryName { id: id.to_string() });
    }
    if !crate::validate::is_valid_key(id) {
        return Err(ParamDefect::InvalidConfig {
            param: SELECT.to_string(),
            field: "id".to_string(),
            expected: "a lowercase identifier, as for a panel key".to_string(),
        });
    }

    required_string(SELECT, config, "label")?;

    let options = required_string(SELECT, config, "options")?;
    if crate::path::normalize(options).is_err() {
        return Err(ParamDefect::InvalidConfig {
            param: SELECT.to_string(),
            field: "options".to_string(),
            expected: "a path relative to the platform base".to_string(),
        });
    }

    match config.get("multiple") {
        None | Some(Value::Bool(_)) => {}
        Some(_) => {
            return Err(ParamDefect::InvalidConfig {
                param: SELECT.to_string(),
                field: "multiple".to_string(),
                expected: "a boolean".to_string(),
            });
        }
    }

    Ok(())
}

fn required_string<'c>(
    param: &str,
    config: &'c Map<String, Value>,
    field: &str,
) -> Result<&'c str, ParamDefect> {
    match config.get(field) {
        Some(Value::String(value)) => Ok(value),
        Some(_) => Err(ParamDefect::InvalidConfig {
            param: param.to_string(),
            field: field.to_string(),
            expected: "a string".to_string(),
        }),
        None => Err(ParamDefect::MissingConfig {
            param: param.to_string(),
            field: field.to_string(),
        }),
    }
}

/// Whether a configuration change narrows what a parameter accepts.
///
/// Narrowing is breaking: a stored selection that used to be valid may no
/// longer be. Widening and cosmetic changes are not. Only the vocabulary can
/// answer this, which is why the diff asks rather than guessing
/// (specification HLIN-S-0001).
///
/// For an unknown parameter there is no basis to judge, so a change is treated
/// as not narrowing; such a panel is already rejected by validation.
pub fn is_narrowing(param: &str, from: &Map<String, Value>, to: &Map<String, Value>) -> bool {
    match param {
        SELECT => {
            let was_multiple = from
                .get("multiple")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let is_multiple = to.get("multiple").and_then(Value::as_bool).unwrap_or(false);
            if was_multiple && !is_multiple {
                return true;
            }
            // The options endpoint is the parameter's value domain, so pointing
            // it somewhere else can invalidate a stored selection just as
            // surely as moving a panel's data endpoint.
            from.get("options") != to.get("options")
        }
        _ => false,
    }
}
