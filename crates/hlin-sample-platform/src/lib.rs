//! A reference platform, from Hlin's side of the boundary.
//!
//! This is what a platform looks like to the shell: a manifest at a well-known
//! path, one endpoint per panel returning a typed envelope, and a health check.
//! It exists to be copied. Everything in it is deliberately ordinary, because
//! it is documentation as much as it is code, and a platform team reading it
//! should find nothing clever to work around.
//!
//! Two things it demonstrates that are easy to miss. Two of its panels read the
//! *same* endpoint, so a surface holding both proves the shell deduplicates
//! rather than fetching twice. And it can be started in either identity mode
//! from [[HLIN-S-0005]], so a demo can show a platform that needed no change
//! beside one that verifies a token.

pub mod changes;
pub mod data;
pub mod manifest;
pub mod routes;

pub use routes::{Config, IdentityMode, router};
