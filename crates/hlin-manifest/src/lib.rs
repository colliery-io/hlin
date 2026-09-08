//! The Hlin contract crate.
//!
//! `hlin-manifest` is the executable form of the contracts that cross the
//! platform/shell boundary. Everything a platform tells Hlin, and everything
//! Hlin can conclude from it, is defined here:
//!
//! - The **manifest** (specification HLIN-S-0001): the document each platform
//!   serves about itself at `/.well-known/hlin.json`, declaring navigation,
//!   panels, the parameters each panel responds to, and lifecycle.
//! - **Contract identity**: the canonical form of what a manifest promises and
//!   the hash over it, which is how the shell notices that a contract moved
//!   without being told.
//! - **Diff classification**: whether a change was safe, and whether the
//!   platform declared it.
//! - The **envelopes** (specification HLIN-S-0002): the typed documents a
//!   panel's data endpoint returns. Envelopes are pure data for a view kind;
//!   panel state travels alongside them in the stream, never inside them.
//!
//! Two properties shape the whole crate. Reading a manifest is *permissive*:
//! unknown fields at every level are preserved rather than rejected, so a
//! platform can add fields ahead of the shell without coordinating an upgrade.
//! Judging one is *total*: no manifest, however wrong, produces a shell error.
//! A document that cannot be used at all degrades the platform to a plain link,
//! and a defect in one panel rejects that panel alone.
//!
//! This crate depends on no other workspace crate, and it knows nothing about
//! how anything is drawn. Rendering belongs to `hlin-view` (decision
//! HLIN-A-0005).
//!
//! ```
//! use hlin_manifest::{classify_diff, contract_hash, parse_str, validate};
//!
//! let document = r#"{
//!   "schema_version": 1,
//!   "contract_version": "1.0.0",
//!   "platform": { "id": "orebank", "name": "Orebank" },
//!   "panels": [{
//!     "key": "queue-depth",
//!     "title": "Queue depth",
//!     "kind": "stat",
//!     "envelope": "scalar.v1",
//!     "data": "api/hlin/queue-depth"
//!   }],
//!   "health": "api/health"
//! }"#;
//!
//! let manifest = parse_str(document).expect("readable");
//! let checked = validate(&manifest, "orebank");
//! assert!(checked.is_document_valid());
//! assert_eq!(checked.accepted_keys(), ["queue-depth"]);
//!
//! // The same promise, written differently, fingerprints the same.
//! let reordered = parse_str(&document.replace(
//!     r#""kind": "stat","#,
//!     r#""kind": "timeseries","#,
//! )).expect("readable");
//! assert_eq!(contract_hash(&manifest), contract_hash(&reordered));
//! assert!(classify_diff(&manifest, &reordered).breaking().is_empty());
//! ```

#![warn(missing_docs)]

// What travels: the five envelope shapes and their limits. Always available,
// because this is all a design pack needs and a design pack should not compile
// a hash function to draw a chart.
pub mod envelope;
pub mod envelope_validate;

// What is promised: the manifest, its fingerprint, and the diff that enforces
// it. Behind the `contract` feature, which is on by default because the shell
// needs every bit of it. A consumer that only draws turns it off and stops
// pulling `sha2`, `semver` and `serde_jcs`, none of which it would ever call.
#[cfg(feature = "contract")]
pub mod canonical;
#[cfg(feature = "contract")]
pub mod diff;
#[cfg(feature = "contract")]
pub mod errors;
#[cfg(feature = "contract")]
pub mod manifest;
#[cfg(feature = "contract")]
pub mod params;
#[cfg(feature = "contract")]
pub mod path;
#[cfg(feature = "contract")]
pub mod validate;

#[cfg(all(test, feature = "contract"))]
mod tests;

pub use envelope::Envelope;
pub use envelope_validate::{EnvelopeDefect, parse_envelope};

#[cfg(feature = "contract")]
pub use canonical::{ContractHash, contract_content, contract_hash};
#[cfg(feature = "contract")]
pub use diff::{Change, Class, DiffReport, Verdict, classify_diff};
#[cfg(feature = "contract")]
pub use errors::{ParseError, parse, parse_str};
#[cfg(feature = "contract")]
pub use manifest::{
    Lifecycle, LifecycleStatus, Manifest, NavigationEntry, Panel, ParamDecl, Platform,
    SUPPORTED_SCHEMA_VERSION, WELL_KNOWN_PATH,
};
#[cfg(feature = "contract")]
pub use validate::{DocumentDefect, PanelDefect, PanelOutcome, Validation, validate};
