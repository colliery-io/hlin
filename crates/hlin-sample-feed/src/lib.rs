//! A reference platform that accepts writes: a feed of posts.
//!
//! [`hlin-sample-platform`] shows what a platform that only answers questions
//! looks like to the shell. This one shows the other half: a platform that
//! people change through its own module, by way of the shell's request proxy
//! ([[HLIN-S-0007]]), and that decides for itself who may change what. It is
//! meant to be copied, so it is deliberately plain.
//!
//! What it demonstrates:
//!
//! - **Authorization belongs to the platform** ([[HLIN-I-0010]] decision 3).
//!   The shell says who is asking, in a signed token, and nothing else. Every
//!   rule here is decided from that token's claims and this platform's own
//!   data: anyone signed in reads; only addresses at one domain may post, which
//!   is the `email` claim; a post's author, by `sub`, may edit and delete it.
//!   See [`rules`].
//! - **A rule that lives nowhere but the platform.** `--muted` names people who
//!   may not post. The shell has never heard of it and could not enforce it,
//!   which is the point: composed platforms each have their own idea of what a
//!   person may do, and Hlin holds no roles.
//! - **Refusals in the platform's own words.** A 403 carries `{ "message" }`
//!   written for a person, and the shell passes it to the module unchanged.
//!   Nothing about the rules is described to the shell in advance (decision 5):
//!   the module may guess, and the platform enforces regardless.
//! - **Writes carry request-bound tokens** (decision 6). A token that is not
//!   bound to this method and this path is refused with a 401, so a read token
//!   lifted from a log cannot be spent on a delete. That check is one extractor
//!   from `hlin-identity`, [`hlin_identity::extract::HlinRequestIdentity`].
//! - **`Idempotency-Key` is honoured.** The shell never retries a write, but a
//!   person may, and the module sends the same key when they do. A write the
//!   platform already made is answered again rather than made twice. See
//!   [`idempotency`].
//! - **An event stream that announces every change**, so another person's
//!   screen updates without polling for it.
//! - **A fallback panel.** The manifest declares `posts` as a shell-drawn
//!   `table`, so the feed still shows something wherever its module cannot be
//!   loaded.
//!
//! [`hlin-sample-platform`]: https://docs.rs/hlin-sample-platform

pub mod idempotency;
pub mod manifest;
pub mod posts;
pub mod routes;
pub mod rules;

pub use routes::{Config, router};
