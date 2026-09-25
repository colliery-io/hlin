//! The bridge majors, outside the `contract` feature.
//!
//! A module's SDK needs this list and nothing else from the manifest, so it
//! lives where a crate built with `default-features = false` can reach it. The
//! manifest re-exports it, so the shell reads the same constant it always did.

/// The bridge majors this shell can host a module on (specification
/// HLIN-S-0007, *Versioning*).
///
/// Here rather than in the shell because both ends of the bridge read it: the
/// shell to refuse a module it cannot speak to, and a module's SDK to say which
/// major it speaks. One list means the two cannot disagree about what `1`
/// means. A major leaves this list only after a deprecation window with a named
/// successor, the way a panel does.
pub const SUPPORTED_BRIDGE_MAJORS: &[u32] = &[1];
