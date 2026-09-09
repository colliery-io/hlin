//! The Hlin composition shell.
//!
//! Hlin discovers what each platform declares it can show, renders every
//! declared panel through one design system, and lets a person arrange panels
//! from any platform into a surface they authored. Nothing crosses the boundary
//! except data and a declared view kind.
//!
//! This crate is the shell itself. Its components, as decomposed in
//! HLIN-I-0001:
//!
//! - **Manifest client** — fetches, parses, and validates one platform's
//!   manifest using the shell's own service identity, never a user's, and
//!   classifies failures as unreachable or malformed.
//! - **Registry** — holds the manifest sequence per platform behind a trait,
//!   computes contract identity, diffs successive contracts, and raises
//!   violations (decision HLIN-A-0002).
//! - **Aggregator** — fans out to panel data endpoints with the viewer's
//!   forwarded identity, deduplicates identical requests per principal
//!   (decision HLIN-A-0004), owns the panel state machine (decision
//!   HLIN-A-0001), and delivers one stream to the browser.
//! - **Layout engine** — user-authored surfaces: selection, arrangement, and
//!   persistence, with layouts personal or org-published and shared read-only
//!   with fork to edit (decision HLIN-A-0007).
//! - **Store** — Postgres, behind a trait that exists for testability rather
//!   than portability (decision HLIN-A-0006). Holds last-seen manifests so
//!   contract enforcement survives a restart, and holds layouts.
//!
//! Rendering lives in `hlin-view`; the contracts live in `hlin-manifest`.

pub mod auth;
pub mod bounded;
pub mod clients;
pub mod config;
pub mod identity;
pub mod layouts;
pub mod manifest_client;
pub mod options;
pub mod registry;
pub mod server;
pub mod store;
pub mod stream;
pub mod surfaces;

/// What Hlin is, in one line. Shown by the binary and by the shell's own
/// about page.
pub const TAGLINE: &str = "One vantage over many systems, assembled by the people who use them.";
