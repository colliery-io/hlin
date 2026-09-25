//! What a platform needs to host a module, in one place.
//!
//! [[HLIN-I-0012]] puts twenty small platforms on one surface, each its own
//! crate, process and module. Each keeps its own rules and state; everything a
//! real platform would also need just to be hosted by Hlin lives here, once,
//! the way `hlin-identity` is shared today. Read top to bottom, this crate is
//! the answer to "what does my platform have to do so the shell can host its
//! module?":
//!
//! 1. **Serve a manifest** ([`Widget::manifest`]) at `/.well-known/hlin.json`,
//!    to anyone, declaring the panel's `ui`, the `assets` prefix its files are
//!    under, the `routes` its module may call through the shell, and its
//!    `events` stream. A shell-drawn fallback (`kind`, `envelope`, `data`)
//!    where the panel has a natural one. Checked with `hlin-manifest` before
//!    the platform will start ([`Widget::defects`]).
//! 2. **Serve the module's files** ([`ModuleFiles`]) under `assets`, to anyone,
//!    hashed names `immutable`: the shell fetches them as itself.
//! 3. **Know who is asking** from the shell's signed token, on every request
//!    ([`Viewer`] for reads), and on a write a token **bound to that method
//!    and path** ([`Write`]), so a read token lifted from a log cannot be spent
//!    on a write.
//! 4. **Honour `Idempotency-Key`** ([`Platform::write`]): the shell never
//!    retries a write, but a person may, and the module resends the same key.
//! 5. **Announce every change on an event stream** (`/api/events`), so another
//!    browser's module refetches without polling.
//! 6. **Refuse in words for a person** ([`Refusal`]): `{ "message" }`, which
//!    the shell passes to the module unchanged.
//!
//! What it does not do is decide anything. Who may do what is each widget's,
//! decided from the token's claims and its own state, in its own code: the
//! shell holds no roles ([[HLIN-I-0010]] decision 3), and neither does this.
//!
//! The module side is `hlin-widget-module`: connecting, fetching again when
//! told, writes with a retry, refusals shown in the platform's words, and the
//! one stylesheet every widget is drawn with.
//!
//! # Adding a widget
//!
//! Copy `crates/widgets/counter`, the smallest widget that writes, and change
//! these, in order:
//!
//! 1. **Rename.** The directory to `crates/widgets/<name>` (without its
//!    `module/dist`); in `Cargo.toml` the package to `hlin-widget-<name>` and
//!    the `[[bin]]` and `[lib]` names to match; in `module/Cargo.toml`,
//!    `hlin-widget-<name>-module`; the `<title>` in `module/index.html`; the
//!    crate name in `src/main.rs` and `tests/rules.rs`. `module/boot.js` and
//!    `module/Trunk.toml` stay as they are.
//! 2. **The rules and state**, in `src/lib.rs`: the state type, its seed, the
//!    [`Widget`] description (`panel = "<name>"`, whether it is `shared`, a
//!    [`Fallback`] or `None` and why), and `api()`, the widget's routes under
//!    `/api/`, reading with [`Platform::read`] and writing with
//!    [`Platform::write`]. Keep the rules as plain functions on the state, so
//!    the tests can read them in one place.
//! 3. **The module**, in `module/src/main.rs`: `start("<name>", ...)`, then
//!    what it fetches (`widget.load`), draws, and writes (`widget.send`). Style it in `module/module.css` with the `--hlin-*`
//!    tokens only ([[HLIN-S-0007]], *theme*), each with a fallback colour.
//! 4. **The tests**, in `tests/rules.rs`: the manifest has no defects, and
//!    each rule, called as the shell calls it (the `testing` module, which
//!    the widget's `[dev-dependencies]` turn on).
//! 5. **The list.** One entry in `WIDGETS` in `.angreal/task_demo.py`:
//!    `{"name": "<name>", "port": 82NN}`, NN being the widget's number in
//!    [[HLIN-I-0012]]'s table. The demo builds its module, starts it, lists it
//!    in the shell's configuration and puts it on "Twenty", in list order.
//! 6. **Check** it: `cargo test -p hlin-widget-<name>`, then
//!    `angreal demo up --with twenty` and `angreal e2e twenty`.
//!
//! Every widget follows the rules in HLIN-T-0080: `src/` is the server and
//! `module/` the Leptos module, both small; it uses this crate for
//! scaffolding and keeps its own rules and state; it declares `ui`, `assets`,
//! `routes` and `events`, and a fallback where one is natural; it is themed
//! only through `--hlin-*`; it announces `changed` after a write (the module
//! does, through the shell) and on its stream (this crate does, on every
//! write); and it has server tests for its rules.

pub mod files;
pub mod idempotency;
pub mod platform;
pub mod serve;
#[cfg(feature = "testing")]
pub mod testing;
pub mod widget;

pub use files::ModuleFiles;
pub use hlin_identity::Claims;
pub use hlin_manifest::envelope;
pub use platform::{Platform, Refusal, Reply, Viewer, Write, router};
pub use serve::{Common, run, serve};
pub use widget::{Fallback, Widget};

/// What a person is called, for showing to other people: their name, or
/// failing that their email, or failing both their id.
pub fn display_name(claims: &Claims) -> String {
    claims
        .name
        .clone()
        .or_else(|| claims.email.clone())
        .unwrap_or_else(|| claims.sub.clone())
}
