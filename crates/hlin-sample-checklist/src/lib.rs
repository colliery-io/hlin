//! A reference platform that people change, from Hlin's side of the boundary.
//!
//! `hlin-sample-platform` shows a platform being looked at. This one shows a
//! platform being *used*: shared checklists that several people add to, cross
//! off and edit, with rules about who may do which. It exists to be copied by
//! a platform team adding writes, so it is deliberately ordinary, and each
//! thing it demonstrates lives in one place:
//!
//! - **The platform authorizes, from the token and its own data**
//!   ([`lists`]). Hlin tells it who is asking (`sub`, `name`, `email`); the
//!   lists, their owners and members are the platform's own, and so are the
//!   rules. The shell holds no roles and is told nothing about them.
//! - **Writes carry identity bound to the request** ([`routes`]). A write's
//!   token names its method and path, and the platform refuses one that does
//!   not match, so a read token cannot be replayed as a change.
//! - **Refusals are the platform's own words**: a 403 with `{"message": ...}`
//!   written for the person who clicked, which their module shows unchanged.
//! - **A retried write is applied once** ([`idempotency`]), by remembering the
//!   answer to each recent `Idempotency-Key`.
//! - **Every change is announced** ([`changes`]) on an event stream, naming
//!   the panel and the list, so any shell watching refetches only that list.
//! - **A fallback anyone can draw** ([`manifest`]): the `items` panel is a
//!   plain `records.v1` table with a `select` of the viewer's lists, so the
//!   list shows even where the platform's own module cannot load.
//!
//! State lives in memory. A restart empties it back to the seeded lists.

pub mod changes;
pub mod idempotency;
pub mod lists;
pub mod manifest;
pub mod routes;

pub use routes::{App, router};
