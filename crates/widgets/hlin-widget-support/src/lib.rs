//! What a platform needs to host a module, in one place.
//!
//! [[HLIN-I-0012]] puts twenty small platforms on one surface, each its own
//! crate, process and module. Each keeps its own rules and state; everything a
//! real platform would also need just to be hosted by Hlin lives here, once,
//! the way `hlin-identity` is shared today. Read top to bottom, this crate is
//! the answer to "what does my platform have to do so the shell can host its
//! module?":
//!
//! 1. **Serve Hlin's surface under one base path** ([`site()`]), `/hlin`
//!    unless configured, nested *before* the UI's catch-all fallback, with a
//!    404 fallback of its own: an unknown path under it is a 404, never the
//!    UI's `index.html`, which a shell asking for a module file would try to
//!    run. The shell's `base_url` for the platform ends in it
//!    (`https://widget.comp.net/hlin`), and every path below is under it.
//! 2. **Serve a manifest** ([`Widget::manifest`]) at `/.well-known/hlin.json`,
//!    to anyone, declaring the panel's `ui`, the `assets` prefix its files are
//!    under, the `routes` its module may call through the shell, and its
//!    `events` stream. A shell-drawn fallback (`kind`, `envelope`, `data`)
//!    where the panel has a natural one. Checked with `hlin-manifest` before
//!    the platform will start ([`Widget::defects`]).
//! 3. **Serve the module's files** ([`ModuleFiles`]) under `assets`, to anyone:
//!    the shell fetches them as itself. Trunk's hashed names
//!    `public, max-age=31536000, immutable`, `index.html` and `boot.js`
//!    `no-cache`, every file an `ETag` and `304` for a match.
//! 4. **Know who is asking** from the shell's signed token, on every request
//!    ([`Viewer`] for reads), and on a write a token **bound to that method
//!    and path** ([`Write`]), so a read token lifted from a log cannot be spent
//!    on a write.
//! 5. **Honour `Idempotency-Key`** ([`Platform::write`]): the shell never
//!    retries a write, but a person may, and the module resends the same key.
//! 6. **Announce every change on an event stream** (`api/events`), so another
//!    browser's module refetches without polling.
//! 7. **Refuse in words for a person** ([`Refusal`]): `{ "message" }`, which
//!    the shell passes to the module unchanged.
//!
//! What it does not do is decide anything. Who may do what is each widget's,
//! decided from the token's claims and its own state, in its own code: the
//! shell holds no roles ([[HLIN-I-0010]] decision 3), and neither does this.
//!
//! # One UI, mounted twice ([[HLIN-I-0013]])
//!
//! A platform has a UI of its own, at the root of its origin, and its Hlin
//! module is **that UI's components, re-exported**, not a second UI. So a
//! widget is five small crates, each with one job:
//!
//! | Folder | Crate | What |
//! |---|---|---|
//! | `src/` | `hlin-widget-<name>` | The server: rules, state, `api()`, `main` |
//! | `components/` | `hlin-widget-<name>-components` | Every component, written against `hlin_widget_ui::use_widget()`, its look in `style.css` |
//! | `ui/` | `hlin-widget-<name>-ui` | `hlin_widget_ui::direct::mount(<Name>)`: the components with the direct client, served at `/` |
//! | `module/` | `hlin-widget-<name>-module` | `pub use` of the components and `hlin_widget_module::mount("<name>", <Name>)`, served under `/hlin/ui/<name>/`. Nothing else |
//!
//! The components never know which client they have. `hlin_widget_ui::Client`
//! is the trait: `fetch`, `attempt` (a write holding one idempotency key for
//! every send), `stream`, `changes`, `announce`, `read_only`, `visible`,
//! `time_range`, `restored` and `on_suspend`. `hlin_widget_ui::direct::Direct`
//! is same-origin `fetch` to `/api/` and the widget's own event stream by
//! `EventSource`; `hlin_widget_module::Hlin` is the SDK's bridge. The
//! components use the `Widget` handle over it (`load`, `send`, `send_then`,
//! `reload`, `loaded_view`), so refusals in the platform's words, *Try again*
//! and suspend behave the same in both.
//!
//! The server answers the same `api()` handlers in two trees ([`site()`]):
//! `/hlin/api/…`, verified with `hlin-token` as always, and `/api/…`, for its
//! own UI, as whoever the platform's own sign-in says. That sign-in is not
//! what this demo designs, so `/api/` acts as one fixed person given by
//! `--local-user`: **demo only**, off by default (`/api/` is then 401), and
//! warned about at start.
//!
//! Both `dist`s are read at start, from `--ui-dir` and `--module-dir`, or from
//! the binary when the widget is built with its `embed` feature
//! ([`dist!`], a single file for a container), or from the repository.
//!
//! **The module's Trunk settings** (`module/Trunk.toml`, `module/index.html`,
//! unchanged from widget to widget): `public_url = "./"`, because the shell
//! serves the module from a path of its own; Trunk's inline loader replaced
//! by `boot.js` through `pattern_script`, because the module CSP refuses
//! inline script; and `data-wasm-opt="z"` with the feature flags rustc emits,
//! which wasm-opt otherwise refuses. The UI's (`ui/`) is an ordinary Trunk
//! site: `public_url = "/"`, the inline loader, the same `data-wasm-opt`.
//!
//! **Leptos 0.8**: its `spawn_local` has no executor until something is
//! mounted, so anything that waits before mounting (the Hlin client's wait for
//! `init`) uses `wasm_bindgen_futures::spawn_local`; and context and view
//! closures demand `Send + Sync`, so a client, which holds `Rc`s, lives in a
//! `StoredValue::new_local`. The components meet neither.
//!
//! # Converting a widget
//!
//! [[HLIN-T-0096]] and [[HLIN-T-0097]] do this to the widgets `angreal demo up
//! --with twenty` does not list as having an own UI. `crates/widgets/counter`
//! is the one to copy; `clock` shows a dependency of its own (`js-sys`),
//! `poll` a component that is not the panel's name.
//!
//! 1. **Move** `module/src/main.rs` to `components/src/lib.rs` and
//!    `module/module.css` to `components/style.css` (`git mv`, so history
//!    follows).
//! 2. **`components/Cargo.toml`**: counter's, named
//!    `hlin-widget-<name>-components`, plus whatever the module needed besides
//!    `hlin-module` and `hlin-widget-module` (`js-sys`, …).
//! 3. **`components/src/lib.rs`**: `start("<name>", |widget: Widget| { … })`
//!    becomes `#[component] pub fn <Name>() -> impl IntoView { let widget =
//!    use_widget(); … view! { <style>{STYLE}</style> {drawn} } }`, with
//!    `pub const STYLE: &str = include_str!("../style.css");`. Imports come
//!    from `hlin_widget_ui` (`Request`, `Loaded`, `loaded_view`,
//!    `use_widget`, `platform_words`), never `hlin_module` or
//!    `hlin_widget_module`. `widget.module().visible()` is
//!    `widget.visible()`; `.context()`'s `time_range` is
//!    `widget.time_range()`; `module.restored()` and `module.on_suspend(…)`
//!    are `widget.restored()` and `widget.on_suspend(…)`; `fetch_stream` is
//!    `widget.stream(request).await`, a `Streamed` whose chunks end in an
//!    `Ended` rather than an `EndError`; a shell refusal arrives already in
//!    words, so `shell_words` goes. Rename a data type the component's name
//!    collides with. Tests of plain functions move with them.
//! 4. **`module/`**: counter's `Cargo.toml` and `src/main.rs` with the names,
//!    panel and component changed; `index.html` with its title; `boot.js` and
//!    `Trunk.toml` stay.
//! 5. **`ui/`**: counter's four files (`Cargo.toml`, `src/main.rs`,
//!    `index.html`, `Trunk.toml`) with the names, component and title changed.
//! 6. **The server**: in `Cargo.toml`, counter's `[features] embed`; in
//!    `src/main.rs`, `run_with(…, Builds { ui: dist!("ui/dist"), module:
//!    dist!("module/dist") })`. `src/lib.rs` and `tests/` do not change.
//! 7. **Check** it: `cargo clippy -p 'hlin-widget-<name>*' --all-targets`,
//!    `angreal demo up --with twenty` (which now builds and serves its UI),
//!    `angreal e2e twenty`, and its own UI at `http://127.0.0.1:82NN/`.
//!    Add `<name>: 82NN` to `OWN_UI` in `e2e/tests/twenty.spec.js`.
//!
//! When no widget is left unconverted: delete `hlin-widget-module`'s
//! `legacy.rs` and its words for the SDK's types, [`run`], and
//! [`Widget::built`].
//!
//! # Adding a widget
//!
//! Copy `crates/widgets/counter`, the smallest widget that writes, and change
//! these, in order:
//!
//! 1. **Rename.** The directory to `crates/widgets/<name>` (without its
//!    `dist`s); `hlin-widget-counter` to `hlin-widget-<name>` in every
//!    `Cargo.toml` (the server's `[[bin]]` and `[lib]` names too), and in
//!    `src/main.rs` and `tests/rules.rs`; the `<title>` in both
//!    `index.html`s.
//! 2. **The rules and state**, in `src/lib.rs`: the state type, its seed, the
//!    [`Widget`] description (`panel = "<name>"`, whether it is `shared`, a
//!    [`Fallback`] or `None` and why), and `api()`, the widget's routes under
//!    `/api/`, reading with [`Platform::read`] and writing with
//!    [`Platform::write`]. Keep the rules as plain functions on the state, so
//!    the tests can read them in one place.
//! 3. **The components**, in `components/src/lib.rs`: what the widget fetches
//!    (`widget.load`), draws, and writes (`widget.send`). Style it in
//!    `components/style.css` with the `--hlin-*` tokens only ([[HLIN-S-0007]],
//!    *theme*), each with a fallback colour.
//! 4. **The tests**, in `tests/rules.rs`: the manifest has no defects, and
//!    each rule, called as the shell calls it (the `testing` module, which
//!    the widget's `[dev-dependencies]` turn on).
//! 5. **The list.** One entry in `WIDGETS` in `.angreal/task_demo.py`:
//!    `{"name": "<name>", "port": 82NN}`, NN being the widget's number in
//!    [[HLIN-I-0012]]'s table. The demo builds its module and UI, starts it,
//!    lists it in the shell's configuration and puts it on "Twenty", in list
//!    order.
//! 6. **Check** it: `cargo test -p hlin-widget-<name>`, then
//!    `angreal demo up --with twenty` and `angreal e2e twenty`.
//!
//! Every widget follows the rules in HLIN-T-0080: `src/` is the server and
//! the components the Leptos UI, both small; it uses this crate for
//! scaffolding and keeps its own rules and state; it declares `ui`, `assets`,
//! `routes` and `events`, and a fallback where one is natural; it is themed
//! only through `--hlin-*`; it announces `changed` after a write (the module
//! does, through the shell) and on its stream (this crate does, on every
//! write); and it has server tests for its rules.

pub mod dist;
pub mod files;
pub mod idempotency;
pub mod platform;
pub mod serve;
pub mod site;
#[cfg(feature = "testing")]
pub mod testing;
pub mod widget;

pub use dist::{Builds, Dist};
pub use files::ModuleFiles;
pub use hlin_identity::Claims;
pub use hlin_manifest::envelope;
pub use platform::{Platform, Refusal, Reply, Viewer, Write};
/// For [`dist!`]: what a widget's `embed` feature compiles its builds in with.
#[cfg(feature = "embed")]
pub use rust_embed;
pub use serve::{Common, run, run_with, serve};
pub use site::{HLIN_BASE, Site, router, site};
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
