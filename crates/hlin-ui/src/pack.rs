//! Holding a design pack without knowing which one it is.
//!
//! `DesignPack` is generic over the view it produces, which is what keeps
//! `hlin-view` free of any UI framework. That generic is useful exactly once,
//! at the moment somebody chooses a pack, and a nuisance everywhere after: a
//! component tree threaded with `P` is a component tree where adding a panel
//! means touching every signature between here and there.
//!
//! So it is erased at the boundary. Whoever mounts the app supplies a pack, the
//! pack draws into whatever view type it likes, and that view becomes an
//! `AnyView` before anything else sees it. Below this module nothing is generic
//! and nothing knows a pack exists.

use std::sync::Arc;

use hlin_view::{DesignPack, RenderPlan};
use leptos::prelude::*;

/// Draws a plan, with a pack chosen by whoever mounted the app.
///
/// Cheap to clone, because Leptos components own their props and this is passed
/// to every panel on a surface.
///
/// `Arc` rather than `Rc`, and the pack required to be `Send + Sync`, because
/// Leptos requires it of anything a reactive closure captures. In a
/// client-rendered frontend nothing is ever sent anywhere, but the bound is the
/// framework's and a pack is almost always a unit struct, so the cost is
/// nothing.
#[derive(Clone)]
pub struct Drawer {
    draws: Arc<Draws>,
    stylesheet: &'static str,
    offers: &'static [&'static str],
}

/// What a pack becomes once its type is gone: a function from a decision to a
/// view, and nothing else.
type Draws = dyn Fn(&RenderPlan<'_>, Option<i64>) -> AnyView + Send + Sync;

impl Drawer {
    /// Take a pack and forget what it was.
    ///
    /// The bound on `P::View` is where a renderer-agnostic trait meets a
    /// Leptos frontend, and it belongs here rather than in `hlin-view`: a pack
    /// that draws to strings is perfectly valid and simply cannot be mounted in
    /// a browser by this crate.
    pub fn of<P>(pack: P) -> Self
    where
        P: DesignPack + Send + Sync + 'static,
        P::View: IntoView + 'static,
    {
        let stylesheet = pack.stylesheet();
        let offers = pack.offers();

        Self {
            draws: Arc::new(move |plan, age| hlin_view::draw(plan, &pack, age).into_any()),
            stylesheet,
            offers,
        }
    }

    /// Draw one panel.
    pub fn draw(&self, plan: &RenderPlan<'_>, age_seconds: Option<i64>) -> AnyView {
        (self.draws)(plan, age_seconds)
    }

    /// The styling the pack's panels need.
    ///
    /// Put on the page by the app rather than fetched from the shell. The shell
    /// serves data and a frontend; which design system that frontend was built
    /// with is not its business, and it used to be told only because the
    /// frontend had no way to hold a pack.
    pub fn stylesheet(&self) -> &'static str {
        self.stylesheet
    }

    /// The components the erased pack answers to.
    ///
    /// Carried alongside the stylesheet for the same reason: the pack itself is
    /// gone by the time anything asks, so anything worth knowing about it has to
    /// be taken while it is still a concrete type.
    pub fn offers(&self) -> &'static [&'static str] {
        self.offers
    }
}

impl std::fmt::Debug for Drawer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Drawer(<pack>)")
    }
}
