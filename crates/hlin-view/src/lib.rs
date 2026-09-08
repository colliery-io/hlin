//! The Hlin view registry.
//!
//! `hlin-view` owns the rendering vocabulary: which view kinds exist, which
//! envelopes each accepts, and what to draw for a panel in any state it can be
//! in. It is a companion crate to the design system rather than part of it
//! (decision HLIN-A-0005), so the design system stays a plain component library
//! and the platform frontends that use it never take on Hlin's contract types.
//!
//! Two properties this crate exists to guarantee, both from specification
//! HLIN-S-0002:
//!
//! - **Totality.** [`plan`] is a total function of `(state, kind, envelope)`.
//!   An unknown kind name falls back to [`Kind::Raw`], every envelope is
//!   accepted by at least `raw`, and every kind has something to draw for every
//!   panel state. No input a platform can produce leaves a panel undrawable,
//!   which is what makes the vision's "rendering is total over its inputs" an
//!   architectural fact rather than a review checklist.
//! - **A governed vocabulary.** Kinds are a bounded set that grows
//!   deliberately, on the design system's cadence. Envelopes grow at platform
//!   speed and their versions are permanent. The acceptance matrix between them
//!   lives here and nowhere else.
//!
//! Adding a kind is a release of this crate followed by a shell redeploy.
//! Platforms never trigger either; they adopt a kind by naming it.
//!
//! The design-system dependency, and the version pin that pairs the two, are
//! added when that crate exists. Until then a [`RenderPlan`] is the seam where
//! its components will attach.
//!
//! ```
//! use hlin_view::{plan, Kind, PanelState, Treatment};
//!
//! // A platform names a kind this shell has never heard of. The panel still
//! // draws, and the layout around it is undisturbed.
//! assert_eq!(Kind::resolve("sankey-diagram"), Kind::Raw);
//!
//! // A person may switch a panel to any kind that accepts its envelope,
//! // which needs a deploy from nobody.
//! assert!(Kind::accepting("series.v1").contains(&Kind::Sparkline));
//!
//! // Every state has a rendering, including the ones with nothing to show.
//! let waiting = plan(PanelState::Loading, Kind::Stat, None);
//! assert_eq!(waiting.treatment, Treatment::Skeleton);
//! ```

#![warn(missing_docs)]

pub mod intent;
pub mod kind;
pub mod pack;
pub mod render;
pub mod state;

pub use intent::{Control, Emit, Intent};
pub use kind::{Kind, accepts};
pub use pack::{Context, DesignPack, draw};
pub use render::{Notice, RenderPlan, Treatment, plan};
pub use state::{Cause, PanelState};

/// The envelope shapes a pack is handed, re-exported from the contract crate.
///
/// Every one of these appears in a [`DesignPack`] method signature, so an
/// implementor needs them. Re-exporting means writing a pack is one dependency
/// rather than two, and means a pack cannot accidentally compile against a
/// different version of the contract than the vocabulary it is implementing.
pub mod envelope {
    pub use hlin_manifest::Envelope;
    pub use hlin_manifest::envelope::{
        Choice, Column, Health, Line, Options, Point, Records, Scalar, Series, Status, StatusItem,
    };
}

pub use hlin_manifest::Envelope;
