//! The shell's second contract: what a browser is told, and when.
//!
//! Specification HLIN-S-0003. The read path only; editing a layout is ordinary
//! request and response.
//!
//! The wire types live in `hlin-stream` so the browser can read the same
//! definitions the shell writes; this module is the machinery around them.

pub mod aggregator;
pub mod events;
pub mod live;
pub mod streams;

/// The wire types, re-exported so shell code has one place to look.
pub use hlin_stream as frame;

pub use aggregator::{Instance, Outcome, Policy, Request, Surface};
pub use hlin_stream::{
    Frame, PROTOCOL_VERSION, PanelFrame, ParamsRequest, SurfaceFrame, TimeRange,
};
