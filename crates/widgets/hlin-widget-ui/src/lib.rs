//! A widget's components, written once and mounted twice ([[HLIN-I-0013]]).
//!
//! Every widget has one set of Leptos components. Its own UI, served at the
//! root of its origin, mounts them with a client that calls its own `/api/`
//! (`direct`, behind the feature of that name). Its Hlin module, served under
//! `/hlin/`, mounts the very same components with a client that goes through
//! the bridge (`hlin_widget_module`). The module has no UI of its own.
//!
//! The components see neither. They start from [`use_widget`], and use the
//! [`Widget`] handle: [`Widget::load`] to fetch and fetch again when told,
//! [`Widget::send`] to write with *Try again*, [`loaded_view`] to draw what
//! came back, [`Widget::read_only`], [`Widget::visible`],
//! [`Widget::time_range`], [`Widget::restored`] and [`Widget::on_suspend`].
//! The handle is written once over [`Client`], which is what each transport
//! implements.
//!
//! [`STYLE`], every widget's stylesheet, is written against the shell's
//! `--hlin-*` tokens only ([[HLIN-S-0007]], *theme*), each with a fallback,
//! which is what the widget's own UI is drawn with, having no shell to send
//! any.

pub mod client;
#[cfg(feature = "direct")]
pub mod direct;
mod widget;

pub use client::{
    Answer, Attempt, Chunks, Client, Ended, Method, Pending, Reply, Request, Streamed, TimeRange,
    Trouble,
};
pub use widget::{Loaded, STYLE, Widget, loaded_view, mount, outcome, platform_words, use_widget};
