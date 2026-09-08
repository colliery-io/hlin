//! Turning a panel's state and data into a plan for drawing it.
//!
//! [`plan`] is the crate's central claim: a *total* function from what the
//! shell knows about a panel to something drawable. It takes no `Result`, it
//! has no panics, and there is no combination of inputs it does not answer.
//! That is the whole reason the vision's "rendering is total over its inputs"
//! is a property of the architecture rather than a habit reviewers have to
//! enforce.
//!
//! What comes back is a description rather than a component. The design system
//! is a separate crate that does not exist yet, and when it arrives this is the
//! seam it attaches to: a [`RenderPlan`] names the treatment and the component
//! family, and the design system supplies the drawing.

use hlin_manifest::Envelope;

use crate::intent::{Control, Emit};
use crate::kind::Kind;
use crate::state::{Cause, PanelState};

/// What to draw for a panel.
///
/// Not `PartialEq`: it carries a callback now, and two plans holding different
/// closures are not usefully different or usefully the same. Nothing compared
/// whole plans anyway — tests compare the fields they mean.
#[derive(Debug, Clone)]
pub struct RenderPlan<'a> {
    /// The kind whose rendering to use. This is the *effective* kind, which may
    /// be [`Kind::Raw`] where the requested one cannot draw what arrived.
    pub kind: Kind,

    /// A component the platform asked for by name, which this crate does not
    /// interpret.
    ///
    /// Hlin's vocabulary is closed, and a design system's own components are
    /// therefore outside it. This carries the name across so a pack can draw
    /// one. Nothing here reads it: the only thing that happens to this string
    /// is being handed to [`DesignPack::custom`], which may decline.
    ///
    /// A pack declining is not a failure. `kind` beside this is a word from the
    /// vocabulary that the acceptance matrix has already cleared against the
    /// envelope, so there is always something to draw.
    pub component: Option<&'a str>,

    /// How the panel should be presented in its current state.
    pub treatment: Treatment,

    /// The document to draw, where the state has one to show.
    pub envelope: Option<&'a Envelope>,

    /// Why the panel is in this state, for the viewer.
    pub notice: Option<Notice>,

    /// The parameters this panel responds to, and what they are set to.
    ///
    /// Empty unless a live surface filled them in, which is also the case for
    /// every panel that declares none.
    pub controls: Vec<Control>,

    /// Where a component sends what a person did.
    ///
    /// Goes nowhere unless a live surface wired it, so a plan built for a test
    /// or a static render still draws components that have handlers on them.
    pub emit: Emit,
}

impl<'a> RenderPlan<'a> {
    /// Whether the plan draws data as opposed to a placeholder or a skeleton.
    pub fn draws_data(&self) -> bool {
        self.envelope.is_some()
    }

    /// Name the component the platform asked for.
    ///
    /// Chained rather than a fourth argument to [`plan`], because a component
    /// is the rare case and [`plan`]'s three arguments are the thing this crate
    /// is about. A caller that has no component says nothing and gets what it
    /// always got.
    pub fn drawn_by(mut self, component: Option<&'a str>) -> Self {
        self.component = component;
        self
    }

    /// Give the panel what it needs to be interacted with.
    ///
    /// One method rather than two because a live surface has both or neither:
    /// the controls say what a component may change, and the emitter is how it
    /// says so. A plan without this draws exactly as it always did, with any
    /// handlers a component wires going nowhere.
    pub fn interactive(mut self, controls: Vec<Control>, emit: Emit) -> Self {
        self.controls = controls;
        self.emit = emit;
        self
    }
}

/// How a panel is presented, independent of which kind draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    /// A skeleton in the shape the kind will fill.
    Skeleton,
    /// The kind's ordinary rendering.
    Normal,
    /// The kind's rendering, visibly aged, with the age shown.
    Aged,
    /// The kind's rendering, dimmed, with the reason shown.
    Dimmed,
    /// No data: a placeholder carrying the reason.
    Placeholder,
}

/// What to tell the viewer about a panel that is not simply working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    /// Why the panel is in this state.
    pub cause: Cause,
    /// Whether to offer the successor the platform named, if the layout has one.
    pub offer_successor: bool,
}

/// Decide what to draw.
///
/// `requested` is the kind to use: the panel's manifest default, or whatever
/// the viewer switched it to. `envelope` is the most recent document, which may
/// be present even in states that will not show it.
///
/// Two fallbacks make this total. A kind that cannot draw what arrived defers
/// to [`Kind::Raw`], which accepts everything, so a manifest declaring a
/// mismatched pairing still yields a readable panel. And a state that shows no
/// data drops the envelope rather than handing it to a kind that would
/// misrepresent it.
pub fn plan<'a>(
    state: PanelState,
    requested: Kind,
    envelope: Option<&'a Envelope>,
) -> RenderPlan<'a> {
    let kind = effective_kind(requested, envelope);
    let showable = state.shows_data().then_some(envelope).flatten();

    let treatment = match state {
        PanelState::Loading => Treatment::Skeleton,
        PanelState::Ready => Treatment::Normal,
        PanelState::Stale => Treatment::Aged,
        PanelState::Unavailable(_) if showable.is_some() => Treatment::Dimmed,
        PanelState::Unavailable(_) => Treatment::Placeholder,
    };

    let notice = match state {
        PanelState::Unavailable(cause) => Some(Notice {
            cause,
            offer_successor: cause.offers_successor(),
        }),
        _ => None,
    };

    RenderPlan {
        kind,
        component: None,
        treatment,
        envelope: showable,
        notice,
        controls: Vec::new(),
        emit: Emit::nowhere(),
    }
}

/// The kind that will actually draw, given what arrived.
///
/// A panel whose declared kind cannot accept its envelope is a manifest the
/// shell should have rejected, so this path should not be reachable through
/// validation. It is here anyway, because "should not be reachable" is not
/// "cannot happen", and a shell that renders everything cannot have a hole
/// where an unexpected pairing goes.
fn effective_kind(requested: Kind, envelope: Option<&Envelope>) -> Kind {
    match envelope {
        Some(document) if !requested.accepts_envelope(document.name()) => Kind::Raw,
        _ => requested,
    }
}
