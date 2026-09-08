//! Classifying what changed between two manifests.
//!
//! This is the mechanism behind the shell acting as its own continuous
//! integration (decision HLIN-A-0002). On each poll the registry compares the
//! manifest it just fetched against the last one it saw. Most polls change
//! nothing. When something does change, this module says whether it was safe,
//! and whether the platform declared it.
//!
//! Two rules keep the mechanism from crying wolf. A version that goes backwards
//! is a rollback, not a violation: rollbacks happen during incidents, exactly
//! when the shell matters most. And a violation never degrades a panel — the
//! shell renders what it was given and tells an operator, because the person
//! looking at a dashboard should not pay for a platform's versioning mistake.
//!
//! Debounce is deliberately *not* implemented here. Whether a change has been
//! observed on enough consecutive polls to be believed is a question about a
//! sequence of manifests, and the registry is what holds the sequence. This
//! module compares exactly two.

use std::collections::BTreeSet;

use semver::Version;

use crate::manifest::{Manifest, Panel, ParamDecl};
use crate::params;

/// How much a change can hurt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Nothing a consumer can pin to moved.
    NonContract,
    /// The contract grew, or grew safer.
    Additive,
    /// Something a consumer could be relying on is gone or narrower.
    Breaking,
}

/// One difference between two manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A panel that did not exist before.
    PanelAdded {
        /// Its key.
        key: String,
    },
    /// A panel that used to exist and does not any more.
    PanelRemoved {
        /// Its key.
        key: String,
        /// Whether it had been deprecated with a window that had not yet ended.
        /// Removing inside the window breaks the promise the window made.
        within_deprecation_window: bool,
    },
    /// A panel's data shape changed, so a consumer's assumptions about it may
    /// no longer hold.
    EnvelopeChanged {
        /// The panel.
        key: String,
        /// What it used to return.
        from: String,
        /// What it returns now.
        to: String,
    },
    /// A panel's data endpoint moved.
    DataChanged {
        /// The panel.
        key: String,
        /// Where it used to be.
        from: String,
        /// Where it is now.
        to: String,
    },
    /// A panel gained a control.
    ParamAdded {
        /// The panel.
        key: String,
        /// The parameter's name.
        param: String,
    },
    /// A panel lost a control it used to respond to.
    ParamRemoved {
        /// The panel.
        key: String,
        /// The parameter's name.
        param: String,
    },
    /// A control's configuration changed in a way that accepts less than it did.
    ParamNarrowed {
        /// The panel.
        key: String,
        /// The parameter's name.
        param: String,
    },
    /// A control's configuration changed in a way that accepts at least as much.
    ParamAdjusted {
        /// The panel.
        key: String,
        /// The parameter's name.
        param: String,
    },
    /// A panel was put on notice.
    Deprecated {
        /// The panel.
        key: String,
    },
    /// A deprecated panel was reprieved.
    Undeprecated {
        /// The panel.
        key: String,
    },
    /// A deprecation window was pushed out.
    SunsetExtended {
        /// The panel.
        key: String,
    },
    /// A deprecation window was pulled in, so consumers have less time than
    /// they were promised.
    SunsetShortened {
        /// The panel.
        key: String,
    },
    /// A panel's default rendering changed. Users may already have chosen a
    /// different kind, so this is not something anyone pins to.
    KindChanged {
        /// The panel.
        key: String,
    },
    /// Cosmetic text changed.
    PresentationChanged {
        /// The panel.
        key: String,
    },
    /// The platform's navigation entries changed.
    NavigationChanged,
    /// The platform's health endpoint moved.
    HealthChanged,
    /// The platform's display identity changed.
    PlatformPresentationChanged,
}

impl Change {
    /// How much this change can hurt.
    pub fn class(&self) -> Class {
        match self {
            Self::PanelRemoved { .. }
            | Self::EnvelopeChanged { .. }
            | Self::DataChanged { .. }
            | Self::ParamRemoved { .. }
            | Self::ParamNarrowed { .. }
            | Self::SunsetShortened { .. } => Class::Breaking,

            Self::PanelAdded { .. }
            | Self::ParamAdded { .. }
            | Self::ParamAdjusted { .. }
            | Self::Deprecated { .. }
            | Self::Undeprecated { .. }
            | Self::SunsetExtended { .. } => Class::Additive,

            Self::KindChanged { .. }
            | Self::PresentationChanged { .. }
            | Self::NavigationChanged
            | Self::HealthChanged
            | Self::PlatformPresentationChanged => Class::NonContract,
        }
    }
}

/// What the shell should do about a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The contract content is identical. Presentation may still have moved.
    NoChange,
    /// The declared version went backwards. Applied and logged, never flagged.
    Rollback {
        /// The version previously seen.
        from: Version,
        /// The version now declared.
        to: Version,
    },
    /// The change is safe, or breakage was declared with a major bump.
    Accepted,
    /// Something broke and nobody said so. The shell renders the manifest as
    /// delivered and raises this for an operator.
    Violation {
        /// The version still being declared.
        declared: Version,
        /// The major version this change needed.
        expected_major: u64,
    },
}

/// Everything that changed between two manifests, and what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// Every difference found, in a stable order.
    pub changes: Vec<Change>,
    /// The worst class among them.
    pub class: Class,
    /// What the shell should do.
    pub verdict: Verdict,
}

impl DiffReport {
    /// Whether anything at all moved.
    pub fn is_unchanged(&self) -> bool {
        self.changes.is_empty()
    }

    /// Every breaking change, for an operator signal that names specifics.
    pub fn breaking(&self) -> Vec<&Change> {
        self.changes
            .iter()
            .filter(|change| change.class() == Class::Breaking)
            .collect()
    }
}

/// Compare the manifest last seen for a platform against the one just fetched.
pub fn classify_diff(previous: &Manifest, next: &Manifest) -> DiffReport {
    let mut changes = platform_changes(previous, next);
    changes.extend(panel_changes(previous, next));

    let class = changes
        .iter()
        .map(Change::class)
        .max()
        .unwrap_or(Class::NonContract);

    let verdict = verdict(previous, next, &changes, class);

    DiffReport {
        changes,
        class,
        verdict,
    }
}

fn verdict(previous: &Manifest, next: &Manifest, changes: &[Change], class: Class) -> Verdict {
    if changes.is_empty() {
        return Verdict::NoChange;
    }
    if next.contract_version < previous.contract_version {
        return Verdict::Rollback {
            from: previous.contract_version.clone(),
            to: next.contract_version.clone(),
        };
    }
    if class == Class::Breaking && next.contract_version.major == previous.contract_version.major {
        return Verdict::Violation {
            declared: next.contract_version.clone(),
            expected_major: previous.contract_version.major + 1,
        };
    }
    Verdict::Accepted
}

fn platform_changes(previous: &Manifest, next: &Manifest) -> Vec<Change> {
    let mut changes = Vec::new();
    if previous.navigation != next.navigation {
        changes.push(Change::NavigationChanged);
    }
    if previous.health != next.health {
        changes.push(Change::HealthChanged);
    }
    if previous.platform.name != next.platform.name || previous.platform.icon != next.platform.icon
    {
        changes.push(Change::PlatformPresentationChanged);
    }
    changes
}

fn panel_changes(previous: &Manifest, next: &Manifest) -> Vec<Change> {
    let mut changes = Vec::new();

    let keys: BTreeSet<&str> = previous
        .panels
        .iter()
        .chain(next.panels.iter())
        .map(|panel| panel.key.as_str())
        .collect();

    for key in keys {
        match (previous.panel(key), next.panel(key)) {
            (None, Some(_)) => changes.push(Change::PanelAdded {
                key: key.to_string(),
            }),
            (Some(before), None) => changes.push(Change::PanelRemoved {
                key: key.to_string(),
                within_deprecation_window: before.lifecycle.is_deprecated(),
            }),
            (Some(before), Some(after)) => changes.extend(one_panel(key, before, after)),
            (None, None) => unreachable!("key came from one of the two manifests"),
        }
    }

    changes
}

fn one_panel(key: &str, before: &Panel, after: &Panel) -> Vec<Change> {
    let mut changes = Vec::new();

    if before.envelope != after.envelope {
        changes.push(Change::EnvelopeChanged {
            key: key.to_string(),
            from: before.envelope.clone(),
            to: after.envelope.clone(),
        });
    }
    if before.data != after.data {
        changes.push(Change::DataChanged {
            key: key.to_string(),
            from: before.data.clone(),
            to: after.data.clone(),
        });
    }
    if before.kind != after.kind {
        changes.push(Change::KindChanged {
            key: key.to_string(),
        });
    }
    if before.title != after.title || before.description != after.description {
        changes.push(Change::PresentationChanged {
            key: key.to_string(),
        });
    }

    changes.extend(param_changes(key, before, after));
    changes.extend(lifecycle_changes(key, before, after));

    changes
}

/// Compare a panel's controls across revisions.
///
/// Parameters are matched by identity rather than by position, so reordering a
/// panel's controls is not a change. A `select` is identified by the query key
/// it claims, which is what makes two selects on one panel distinguishable.
fn param_changes(key: &str, before: &Panel, after: &Panel) -> Vec<Change> {
    let mut changes = Vec::new();

    let find = |panel: &Panel, identity: &(String, Option<String>)| -> Option<ParamDecl> {
        panel
            .params
            .iter()
            .find(|declaration| &declaration.identity() == identity)
            .cloned()
    };

    let mut identities: Vec<(String, Option<String>)> = Vec::new();
    for declaration in before.params.iter().chain(after.params.iter()) {
        let identity = declaration.identity();
        if !identities.contains(&identity) {
            identities.push(identity);
        }
    }
    identities.sort();

    for identity in identities {
        match (find(before, &identity), find(after, &identity)) {
            (None, Some(declaration)) => changes.push(Change::ParamAdded {
                key: key.to_string(),
                param: declaration.param,
            }),
            (Some(declaration), None) => changes.push(Change::ParamRemoved {
                key: key.to_string(),
                param: declaration.param,
            }),
            (Some(was), Some(now)) if was.config != now.config => {
                if params::is_narrowing(&now.param, &was.config, &now.config) {
                    changes.push(Change::ParamNarrowed {
                        key: key.to_string(),
                        param: now.param,
                    });
                } else {
                    changes.push(Change::ParamAdjusted {
                        key: key.to_string(),
                        param: now.param,
                    });
                }
            }
            _ => {}
        }
    }

    changes
}

fn lifecycle_changes(key: &str, before: &Panel, after: &Panel) -> Vec<Change> {
    let mut changes = Vec::new();

    match (
        before.lifecycle.is_deprecated(),
        after.lifecycle.is_deprecated(),
    ) {
        (false, true) => changes.push(Change::Deprecated {
            key: key.to_string(),
        }),
        (true, false) => changes.push(Change::Undeprecated {
            key: key.to_string(),
        }),
        _ => {}
    }

    if let (Some(was), Some(now)) = (before.lifecycle.sunset, after.lifecycle.sunset) {
        if now < was {
            changes.push(Change::SunsetShortened {
                key: key.to_string(),
            });
        } else if now > was {
            changes.push(Change::SunsetExtended {
                key: key.to_string(),
            });
        }
    }

    changes
}
