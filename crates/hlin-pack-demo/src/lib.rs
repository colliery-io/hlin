//! A small design pack, for the demo.
//!
//! Implements [`hlin_view::DesignPack`] with plain Leptos and hand-rolled SVG.
//! It exists so Hlin can render before the shared design system is written,
//! and it is deliberately plain: useful enough to read, unstyled enough that
//! nobody mistakes it for a design.
//!
//! It is also the worked example of what a pack is. A real one implements the
//! same trait and is selected in the shell's build, and nothing in `hlin-view`
//! refers to either (decision HLIN-A-0005, refined by HLIN-I-0002).

#![warn(missing_docs)]

pub mod chart;
pub mod format;
mod panels;

use hlin_manifest::Envelope;
use hlin_manifest::envelope::{Options, Records, Scalar, Series, Status};
use hlin_view::pack::Context;
use hlin_view::{DesignPack, Kind};
use leptos::prelude::*;

/// The stylesheet this pack needs.
///
/// Scoped under one class so it cannot leak into a host page, and served by
/// the shell alongside the frontend.
pub const STYLESHEET: &str = include_str!("pack.css");

/// The class every panel this pack draws is wrapped in.
pub const ROOT_CLASS: &str = "hlin-pack-demo";

/// The demo design pack.
#[derive(Debug, Clone, Copy, Default)]
pub struct DemoPack;

impl DesignPack for DemoPack {
    type View = AnyView;

    fn stat(&self, data: &Scalar, context: Context) -> Self::View {
        panels::stat(data, context)
    }

    fn timeseries(&self, data: &Series, context: Context) -> Self::View {
        panels::timeseries(data, context)
    }

    fn sparkline(&self, data: &Series, context: Context) -> Self::View {
        panels::sparkline(data, context)
    }

    fn table(&self, data: &Records, context: Context) -> Self::View {
        panels::table(data, context)
    }

    fn series_as_table(&self, data: &Series, context: Context) -> Self::View {
        panels::series_as_table(data, context)
    }

    fn status(&self, data: &Status, context: Context) -> Self::View {
        panels::status(data, context)
    }

    fn options(&self, data: &Options, context: Context) -> Self::View {
        panels::options(data, context)
    }

    fn raw(&self, data: &Envelope, context: Context) -> Self::View {
        panels::raw(data, context)
    }

    fn offers(&self) -> &'static [&'static str] {
        &[]
    }

    /// This pack offers no components beyond the vocabulary, and says so.
    ///
    /// Declining is the interesting half of the feature rather than a gap in
    /// this pack: a platform can ask for a component a real design system has,
    /// and a surface drawn by this one still works, showing the declared kind.
    /// That is what makes naming a component safe for a platform that does not
    /// know which pack a given shell mounted.
    fn custom(&self, _component: &str, _data: &Envelope, _context: Context) -> Option<Self::View> {
        None
    }

    fn skeleton(&self, kind: Kind, context: Context) -> Self::View {
        panels::skeleton(kind, context)
    }

    fn placeholder(&self, kind: Kind, context: Context) -> Self::View {
        panels::placeholder(kind, context)
    }

    fn stylesheet(&self) -> &'static str {
        STYLESHEET
    }
}
