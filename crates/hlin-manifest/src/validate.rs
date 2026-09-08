//! Checking a manifest, one panel at a time.
//!
//! Validation is deliberately two-tiered (specification HLIN-S-0001 REQ-3.2).
//! A defect that makes the document unusable as a whole degrades the platform
//! to a plain navigation link; a defect in one panel declaration rejects that
//! panel and nothing else. One typo must not take down a platform's other
//! panels.
//!
//! Nothing here returns an error in the ordinary sense. A manifest problem is
//! never a shell error: it is a classified outcome that the shell renders. The
//! reasons are structured so the registry can act on them without parsing a
//! message.

use std::collections::BTreeMap;
use std::fmt;

use crate::manifest::{LifecycleStatus, Manifest, Panel, SUPPORTED_SCHEMA_VERSION};
use crate::params::{self, ParamDefect};
use crate::path::{self, PathDefect};

/// Why a manifest cannot be used at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentDefect {
    /// `schema_version` was below 1, which no version of the format has used.
    InvalidSchemaVersion {
        /// What the document declared.
        declared: u32,
    },
    /// The manifest claims to belong to a platform other than the one whose
    /// base it was fetched from. Shell configuration is authoritative.
    PlatformIdMismatch {
        /// The identity the document claimed.
        declared: String,
        /// The identity configuration assigns to that base.
        expected: String,
    },
    /// `platform.id` is not a usable identifier.
    InvalidPlatformId {
        /// The identity the document claimed.
        declared: String,
    },
    /// The health endpoint could not be resolved against the platform base.
    InvalidHealthPath {
        /// The path as declared.
        declared: String,
        /// Why it was rejected.
        defect: PathDefect,
    },
}

impl fmt::Display for DocumentDefect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSchemaVersion { declared } => {
                write!(formatter, "schema_version {declared} is below 1")
            }
            Self::PlatformIdMismatch { declared, expected } => write!(
                formatter,
                "manifest declares platform `{declared}` but this base is configured as `{expected}`"
            ),
            Self::InvalidPlatformId { declared } => {
                write!(formatter, "`{declared}` is not a usable platform id")
            }
            Self::InvalidHealthPath { declared, defect } => {
                write!(formatter, "health path `{declared}`: {defect}")
            }
        }
    }
}

/// Why one panel declaration cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelDefect {
    /// The key is not a usable identifier.
    InvalidKey {
        /// The key as declared.
        declared: String,
    },
    /// More than one panel claimed this key. Every claimant is rejected: the
    /// shell does not guess which one was meant.
    DuplicateKey {
        /// The contested key.
        key: String,
    },
    /// A required field was present but empty.
    EmptyField {
        /// Which field.
        field: &'static str,
    },
    /// The data endpoint could not be resolved against the platform base.
    InvalidDataPath {
        /// The path as declared.
        declared: String,
        /// Why it was rejected.
        defect: PathDefect,
    },
    /// The panel declared an envelope that is not a panel envelope. `options.v1`
    /// is the shape a `select` control reads, and no view kind draws one.
    NotAPanelEnvelope {
        /// The envelope the panel declared.
        declared: String,
    },
    /// A parameter declaration was unusable.
    InvalidParam(ParamDefect),
    /// The same parameter was declared twice on one panel.
    DuplicateParam {
        /// The parameter name.
        param: String,
        /// Its query key, where the parameter has one.
        id: Option<String>,
    },
    /// A deprecated panel did not say when its window ends.
    MissingSunset,
    /// A sunset date was given for a panel that is not deprecated.
    UnexpectedSunset,
    /// The named successor is not a panel in this manifest.
    UnknownSuccessor {
        /// The key the panel pointed at.
        successor: String,
    },
    /// A panel named itself as its own successor.
    SelfSuccessor,
}

impl fmt::Display for PanelDefect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey { declared } => {
                write!(formatter, "`{declared}` is not a usable panel key")
            }
            Self::DuplicateKey { key } => {
                write!(formatter, "panel key `{key}` is declared more than once")
            }
            Self::EmptyField { field } => write!(formatter, "`{field}` is empty"),
            Self::InvalidDataPath { declared, defect } => {
                write!(formatter, "data path `{declared}`: {defect}")
            }
            Self::NotAPanelEnvelope { declared } => write!(
                formatter,
                "`{declared}` is not an envelope a panel can declare"
            ),
            Self::InvalidParam(defect) => write!(formatter, "parameter: {defect:?}"),
            Self::DuplicateParam {
                param,
                id: Some(id),
            } => {
                write!(
                    formatter,
                    "parameter `{param}` with id `{id}` is declared more than once"
                )
            }
            Self::DuplicateParam { param, id: None } => {
                write!(formatter, "parameter `{param}` is declared more than once")
            }
            Self::MissingSunset => formatter.write_str("deprecated panels must declare a sunset"),
            Self::UnexpectedSunset => {
                formatter.write_str("a sunset was declared for a panel that is not deprecated")
            }
            Self::UnknownSuccessor { successor } => {
                write!(
                    formatter,
                    "successor `{successor}` is not a panel in this manifest"
                )
            }
            Self::SelfSuccessor => formatter.write_str("a panel cannot be its own successor"),
        }
    }
}

/// What became of one declared panel.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelOutcome {
    /// Position in the manifest's `panels` array, so a rejected panel can be
    /// pointed at even when its key is the thing that is wrong.
    pub index: usize,
    /// The key it declared.
    pub key: String,
    /// Why it was rejected, if it was.
    pub defect: Option<PanelDefect>,
}

impl PanelOutcome {
    /// Whether this panel is usable.
    pub fn is_accepted(&self) -> bool {
        self.defect.is_none()
    }
}

/// The result of checking a manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Validation {
    /// Set when the document is unusable as a whole. When set, no panels were
    /// examined: the platform degrades to a plain link.
    pub document: Option<DocumentDefect>,
    /// One outcome per declared panel, in declaration order.
    pub panels: Vec<PanelOutcome>,
    /// Set when the manifest declares a newer format than this shell knows.
    /// Informational: known fields are honoured and unknown ones ignored.
    pub newer_schema_version: Option<u32>,

    /// Set when an event stream was declared and its path cannot be used.
    ///
    /// Informational, like the field above, and deliberately *not* a
    /// [`DocumentDefect`]. A bad `health` path rejects the document because the
    /// shell cannot tell whether the platform is alive; a bad `events` path
    /// costs nothing but the acceleration, because the shell was already
    /// correct without it (HLIN-S-0006 REQ-1.2).
    ///
    /// Rejecting the document would mean that adding an optional field wrong
    /// takes an entire platform offline — which would make this feature more
    /// dangerous to adopt than to skip, and is exactly backwards.
    pub unusable_events: Option<PathDefect>,
}

impl Validation {
    /// Whether the document itself is usable.
    pub fn is_document_valid(&self) -> bool {
        self.document.is_none()
    }

    /// The keys of every panel that may be offered to users.
    pub fn accepted_keys(&self) -> Vec<&str> {
        self.panels
            .iter()
            .filter(|outcome| outcome.is_accepted())
            .map(|outcome| outcome.key.as_str())
            .collect()
    }

    /// Every panel that was rejected, with its reason.
    pub fn rejected(&self) -> Vec<(&str, &PanelDefect)> {
        self.panels
            .iter()
            .filter_map(|outcome| {
                outcome
                    .defect
                    .as_ref()
                    .map(|defect| (outcome.key.as_str(), defect))
            })
            .collect()
    }
}

/// Check a manifest fetched from the base that configuration assigns to
/// `expected_platform_id`.
///
/// Validation needs no other platform's manifest and no shell state beyond the
/// vocabularies (NFR-1.1), so it is a pure function of the document and the
/// identity the shell expected.
pub fn validate(manifest: &Manifest, expected_platform_id: &str) -> Validation {
    let newer_schema_version =
        (manifest.schema_version > SUPPORTED_SCHEMA_VERSION).then_some(manifest.schema_version);

    if let Some(defect) = document_defect(manifest, expected_platform_id) {
        return Validation {
            document: Some(defect),
            panels: Vec::new(),
            newer_schema_version,
            unusable_events: None,
        };
    }

    Validation {
        document: None,
        panels: panel_outcomes(manifest),
        newer_schema_version,
        unusable_events: unusable_events(manifest),
    }
}

/// Whether a declared event stream can be resolved against the platform base.
///
/// The same rule every other path in a manifest is held to (HLIN-S-0001
/// REQ-1.2): relative, no absolute or scheme-relative URLs, no `..`. A manifest
/// must never be able to point the shell outside the base it came from, and an
/// event stream is a held-open connection, which makes that matter more here
/// rather than less.
fn unusable_events(manifest: &Manifest) -> Option<PathDefect> {
    path::normalize(manifest.events.as_deref()?).err()
}

fn document_defect(manifest: &Manifest, expected_platform_id: &str) -> Option<DocumentDefect> {
    if manifest.schema_version < 1 {
        return Some(DocumentDefect::InvalidSchemaVersion {
            declared: manifest.schema_version,
        });
    }
    if !is_valid_key(&manifest.platform.id) {
        return Some(DocumentDefect::InvalidPlatformId {
            declared: manifest.platform.id.clone(),
        });
    }
    if manifest.platform.id != expected_platform_id {
        return Some(DocumentDefect::PlatformIdMismatch {
            declared: manifest.platform.id.clone(),
            expected: expected_platform_id.to_string(),
        });
    }
    if let Err(defect) = path::normalize(&manifest.health) {
        return Some(DocumentDefect::InvalidHealthPath {
            declared: manifest.health.clone(),
            defect,
        });
    }
    None
}

fn panel_outcomes(manifest: &Manifest) -> Vec<PanelOutcome> {
    let mut claims: BTreeMap<&str, usize> = BTreeMap::new();
    for panel in &manifest.panels {
        *claims.entry(panel.key.as_str()).or_insert(0) += 1;
    }

    let declared_keys: Vec<&str> = manifest
        .panels
        .iter()
        .map(|panel| panel.key.as_str())
        .collect();

    manifest
        .panels
        .iter()
        .enumerate()
        .map(|(index, panel)| {
            let contested = claims
                .get(panel.key.as_str())
                .is_some_and(|count| *count > 1);
            let defect = if contested {
                Some(PanelDefect::DuplicateKey {
                    key: panel.key.clone(),
                })
            } else {
                panel_defect(panel, &declared_keys)
            };
            PanelOutcome {
                index,
                key: panel.key.clone(),
                defect,
            }
        })
        .collect()
}

fn panel_defect(panel: &Panel, declared_keys: &[&str]) -> Option<PanelDefect> {
    if !is_valid_key(&panel.key) {
        return Some(PanelDefect::InvalidKey {
            declared: panel.key.clone(),
        });
    }
    if panel.title.trim().is_empty() {
        return Some(PanelDefect::EmptyField { field: "title" });
    }
    if panel.kind.trim().is_empty() {
        return Some(PanelDefect::EmptyField { field: "kind" });
    }
    if panel.envelope.trim().is_empty() {
        return Some(PanelDefect::EmptyField { field: "envelope" });
    }
    if let Err(defect) = path::normalize(&panel.data) {
        return Some(PanelDefect::InvalidDataPath {
            declared: panel.data.clone(),
            defect,
        });
    }

    // An envelope this shell has never heard of is not rejected here: the
    // vocabulary grows, and a panel naming a newer envelope should fail when
    // its data arrives rather than vanish from the picker. What is rejected is
    // a name the shell knows to be the wrong *kind* of thing.
    if let Some(crate::envelope::Role::Parameter) = crate::envelope::role(&panel.envelope) {
        return Some(PanelDefect::NotAPanelEnvelope {
            declared: panel.envelope.clone(),
        });
    }

    let mut seen: Vec<(String, Option<String>)> = Vec::new();
    for declaration in &panel.params {
        if let Err(defect) = params::validate(&declaration.param, &declaration.config) {
            return Some(PanelDefect::InvalidParam(defect));
        }
        let identity = declaration.identity();
        if seen.contains(&identity) {
            return Some(PanelDefect::DuplicateParam {
                param: identity.0,
                id: identity.1,
            });
        }
        seen.push(identity);
    }

    match (panel.lifecycle.status, panel.lifecycle.sunset) {
        (LifecycleStatus::Deprecated, None) => return Some(PanelDefect::MissingSunset),
        (LifecycleStatus::Active, Some(_)) => return Some(PanelDefect::UnexpectedSunset),
        _ => {}
    }

    if let Some(successor) = &panel.lifecycle.successor {
        if successor == &panel.key {
            return Some(PanelDefect::SelfSuccessor);
        }
        if !declared_keys.contains(&successor.as_str()) {
            return Some(PanelDefect::UnknownSuccessor {
                successor: successor.clone(),
            });
        }
    }

    None
}

/// Whether a string is a usable identifier for a platform, a panel, or a
/// parameter's query key.
///
/// The pattern is `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$`: lowercase alphanumerics
/// and hyphens, two to sixty-four characters, never starting or ending with a
/// hyphen. Identifiers appear in URLs, in layout references and in query
/// strings, so the set stays small enough to be unambiguous everywhere.
pub fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.len() < 2 || bytes.len() > 64 {
        return false;
    }
    let is_alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !is_alphanumeric(bytes[0]) || !is_alphanumeric(bytes[bytes.len() - 1]) {
        return false;
    }
    bytes[1..bytes.len() - 1]
        .iter()
        .all(|byte| is_alphanumeric(*byte) || *byte == b'-')
}
