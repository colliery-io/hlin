//! The seam between deciding what to draw and drawing it.
//!
//! [`crate::plan`] decides; a *design pack* draws. This module is the contract
//! between them, and it is what lets the two move independently: the shared
//! design system ships a pack, the demo ships a smaller one, and a platform
//! with a reason to look different could ship a third. None of them are
//! referenced from here (decision HLIN-A-0005, as refined by HLIN-I-0002).
//!
//! The trait is generic over its view type and this crate takes no UI
//! dependency, so the vocabulary and its totality test stay a property of the
//! contract rather than of whatever draws it.
//!
//! Every method is required. A pack that forgets a kind does not compile, which
//! is the same guarantee `plan` gives from the other direction: between them,
//! there is no panel state and no envelope that reaches a hole.

use hlin_manifest::Envelope;
use hlin_manifest::envelope::{Options, Records, Scalar, Series, Status};

use crate::intent::{Control, Emit};
use crate::kind::Kind;
use crate::render::{Notice, RenderPlan, Treatment};

/// What a pack is told about the panel it is drawing.
///
/// Passed alongside the data so a component never has to consult the plan
/// itself, and so adding context later does not change every method signature.
#[derive(Debug, Clone)]
pub struct Context {
    /// How this panel should be presented.
    pub treatment: Treatment,
    /// Why it is in this state, where there is a reason worth showing.
    pub notice: Option<Notice>,
    /// How old the data is, in seconds, where the shell knows.
    pub age_seconds: Option<i64>,
    /// The parameters this panel responds to and what they are set to.
    ///
    /// Empty for the many panels that declare none. A component drawing its
    /// own filter reads this rather than being told separately, so a pack that
    /// ignores parameters needs to know nothing about them.
    pub controls: Vec<Control>,
    /// Where to send what a person did.
    ///
    /// Always callable. Outside a live surface it goes nowhere, so a component
    /// can wire its handlers without asking whether anyone is listening —
    /// though [`Emit::is_live`] is there for a component that would rather draw
    /// something static than something inert.
    pub emit: Emit,
}

impl Context {
    /// A panel drawing fresh data with nothing to explain.
    ///
    /// No controls and nothing listening, which is what a test or a static
    /// render wants. A caller with either fills them in afterwards.
    pub fn ready() -> Self {
        Self {
            treatment: Treatment::Normal,
            notice: None,
            age_seconds: None,
            controls: Vec::new(),
            emit: Emit::nowhere(),
        }
    }

    /// The control with this id, where the panel declares one.
    pub fn control(&self, id: &str) -> Option<&Control> {
        self.controls.iter().find(|control| control.id == id)
    }

    /// Whether the pack should visibly mark this panel as not current.
    pub fn is_aged(&self) -> bool {
        matches!(self.treatment, Treatment::Aged | Treatment::Dimmed)
    }
}

/// Something that can draw Hlin's panels.
///
/// Implement this to bring a design system to the shell.
pub trait DesignPack {
    /// Whatever this pack produces: a Leptos view, an HTML string, a test
    /// double that records calls.
    type View;

    /// One value, with its unit, label and any delta.
    fn stat(&self, data: &Scalar, context: Context) -> Self::View;

    /// A line chart over time, with a legend, and gaps drawn as gaps.
    fn timeseries(&self, data: &Series, context: Context) -> Self::View;

    /// A compact line per series with its latest value, and no axes.
    fn sparkline(&self, data: &Series, context: Context) -> Self::View;

    /// A sortable table of records.
    fn table(&self, data: &Records, context: Context) -> Self::View;

    /// A time series laid out as a table: one row per instant, one column per
    /// series.
    ///
    /// Separate from [`DesignPack::table`] because the data is a different
    /// shape, and the acceptance matrix lets `table` accept both.
    fn series_as_table(&self, data: &Series, context: Context) -> Self::View;

    /// A health rollup, with its parts where there are any.
    fn status(&self, data: &Status, context: Context) -> Self::View;

    /// The choices a parameter offers, drawn as a panel.
    ///
    /// Only `raw` accepts `options.v1`, since it is read by a control rather
    /// than shown as a panel, but a pack must still have something to draw if
    /// one arrives.
    fn options(&self, data: &Options, context: Context) -> Self::View;

    /// Any document, drawn plainly: the envelope's name and its contents as a
    /// readable tree.
    ///
    /// This is what makes rendering total. It is where an unknown kind lands,
    /// where a mismatched pairing lands, and the guaranteed rendering for
    /// every envelope. It should be useful enough to read and plain enough
    /// that nobody ships it deliberately.
    fn raw(&self, data: &Envelope, context: Context) -> Self::View;

    /// A component this design system offers that Hlin has no word for.
    ///
    /// `component` is a name a platform put in its manifest and Hlin forwarded
    /// without reading. It is the design system's own vocabulary — `Meter`,
    /// `Graph`, whatever this pack calls things — and it exists because Hlin's
    /// six kinds are a closed set that a design system's components will always
    /// exceed.
    ///
    /// Returning `None` is the ordinary answer and is not a failure: it means
    /// this pack does not offer that component, and the caller draws the
    /// panel's declared kind instead. That kind has already been checked
    /// against the envelope, so declining is always safe.
    ///
    /// There is no default implementation, like everything else here. A pack
    /// that extends nothing writes `None` and means it, rather than inheriting
    /// a decision it never made.
    fn custom(&self, component: &str, data: &Envelope, context: Context) -> Option<Self::View>;

    /// The component names this pack answers to.
    ///
    /// Advisory, and deliberately so: it must not gate [`DesignPack::custom`].
    /// A pack that listed a component it then declined would produce a panel
    /// nothing draws, which is precisely the hole the vocabulary exists to
    /// prevent. `custom` remains the authority on what actually draws; this
    /// exists only so somebody can be told.
    ///
    /// What it buys is the difference between a typo and an unsupported
    /// component, which are otherwise indistinguishable. A platform declaring
    /// `aurora.grpah` gets its declared kind, silently and forever, exactly as
    /// a shell running a pack without that component would — so the fallback
    /// that makes component naming safe is also what hides a mistake in it.
    ///
    /// Required, like everything else here. A pack that extends nothing returns
    /// `&[]` and means it, rather than inheriting a claim it never made.
    fn offers(&self) -> &'static [&'static str];

    /// Nothing has arrived yet: the shape this kind will fill.
    fn skeleton(&self, kind: Kind, context: Context) -> Self::View;

    /// Nothing can be shown, and here is why.
    fn placeholder(&self, kind: Kind, context: Context) -> Self::View;

    /// The styling these views need to look like themselves.
    ///
    /// Part of the contract rather than a convention, because a pack whose CSS
    /// has to be wired up separately is a pack that can be installed wrongly. A
    /// frontend puts this on the page and needs to know nothing else about how
    /// the panels are drawn.
    ///
    /// `&'static str` because a stylesheet is a compile-time asset in every
    /// pack anybody has written: `include_str!`, or a `concat!` of several. A
    /// pack that genuinely builds its CSS at runtime would need this widened,
    /// and none does.
    ///
    /// Required, like every other method here, but trivially satisfied: a pack
    /// with no styling of its own returns `""`.
    ///
    /// # Filling the shell's chrome
    ///
    /// The frame Hlin draws around these panels — the bar, the picker, the
    /// grid — is Hlin's own and asks for its colours by role, as CSS custom
    /// properties. A pack that sets them makes the frame match its panels; a
    /// pack that ignores them still works, on the shell's own defaults. Setting
    /// them is a few lines at the top of a stylesheet:
    ///
    /// ```css
    /// :root {
    ///   --hlin-surface:   /* the page behind everything */;
    ///   --hlin-raised:    /* bars, panels, inputs */;
    ///   --hlin-border:    /* every line between two things */;
    ///   --hlin-text:      /* ordinary reading text */;
    ///   --hlin-dim:       /* labels and captions */;
    ///   --hlin-faint:     /* affordances at rest, like an unheld drag handle */;
    ///   --hlin-accent:    /* the one colour meaning "this, here" */;
    ///   --hlin-on-accent: /* text legible on top of the accent */;
    ///   --hlin-good:      /* connected */;
    ///   --hlin-warn:      /* waiting */;
    ///   --hlin-bad:       /* wrong */;
    /// }
    /// ```
    ///
    /// That list is deliberately short and is not a theming system. Every
    /// property on it is one more thing a design system has to answer for, and
    /// a frame that needed twenty of them would be a frame that should have
    /// been the pack's to draw.
    fn stylesheet(&self) -> &'static str;
}

/// Draw a plan with a pack.
///
/// Total by construction: every combination of kind, treatment and envelope
/// presence reaches exactly one method, and there is no path that returns an
/// error or panics. If this function ever needs a fallback arm that is not
/// [`DesignPack::raw`], something upstream has stopped being total.
pub fn draw<P: DesignPack>(plan: &RenderPlan<'_>, pack: &P, age_seconds: Option<i64>) -> P::View {
    let context = Context {
        treatment: plan.treatment,
        notice: plan.notice,
        age_seconds,
        controls: plan.controls.clone(),
        emit: plan.emit.clone(),
    };

    let Some(envelope) = plan.envelope else {
        return match plan.treatment {
            Treatment::Skeleton => pack.skeleton(plan.kind, context),
            _ => pack.placeholder(plan.kind, context),
        };
    };

    // A component the platform named gets first refusal. The pack answers
    // `None` when it does not offer one by that name, and the ordinary
    // vocabulary below draws the panel instead — which is why naming a
    // component cannot make a panel undrawable, and why this needs no
    // validation anywhere: an unknown name costs one method call.
    if let Some(component) = plan.component
        // Cloned because `custom` may decline, and the vocabulary below then
        // needs the context that the attempt would otherwise have consumed. An
        // `Arc` bump and a small `Vec`, once per panel per frame.
        && let Some(view) = pack.custom(component, envelope, context.clone())
    {
        return view;
    }

    match (plan.kind, envelope) {
        (Kind::Stat, Envelope::Scalar(data)) => pack.stat(data, context),
        (Kind::Timeseries, Envelope::Series(data)) => pack.timeseries(data, context),
        (Kind::Sparkline, Envelope::Series(data)) => pack.sparkline(data, context),
        (Kind::Table, Envelope::Records(data)) => pack.table(data, context),
        (Kind::Table, Envelope::Series(data)) => pack.series_as_table(data, context),
        (Kind::Status, Envelope::Status(data)) => pack.status(data, context),
        (Kind::Raw, Envelope::Options(data)) => pack.options(data, context),

        // Everything else is `raw`, which accepts every envelope. `plan` has
        // already redirected a kind that cannot draw what arrived, so reaching
        // here with a mismatch means the plan was built by hand; drawing it
        // plainly is still better than refusing.
        (_, document) => pack.raw(document, context),
    }
}

/// A boxed pack is a pack.
///
/// [`DesignPack`] carries an associated view type, which usually costs object
/// safety and here does not: with the view named, `dyn DesignPack<View = V>` is
/// a type you can hold. This impl is what makes that useful, because a bare
/// trait object does not implement the trait it is a object of, so without it
/// every caller taking `P: DesignPack` would need a second entry point for the
/// boxed case.
///
/// What it buys is choosing a pack at runtime rather than at build time: a
/// frontend can hold several and pick one. That is a demonstration rather than
/// a deployment pattern — every pack in the box is compiled into the binary and
/// paid for — but it costs eleven lines of forwarding to have available.
impl<V> DesignPack for Box<dyn DesignPack<View = V> + Send + Sync> {
    type View = V;

    fn stat(&self, data: &Scalar, context: Context) -> V {
        (**self).stat(data, context)
    }
    fn timeseries(&self, data: &Series, context: Context) -> V {
        (**self).timeseries(data, context)
    }
    fn sparkline(&self, data: &Series, context: Context) -> V {
        (**self).sparkline(data, context)
    }
    fn table(&self, data: &Records, context: Context) -> V {
        (**self).table(data, context)
    }
    fn series_as_table(&self, data: &Series, context: Context) -> V {
        (**self).series_as_table(data, context)
    }
    fn status(&self, data: &Status, context: Context) -> V {
        (**self).status(data, context)
    }
    fn options(&self, data: &Options, context: Context) -> V {
        (**self).options(data, context)
    }
    fn custom(&self, component: &str, data: &Envelope, context: Context) -> Option<V> {
        (**self).custom(component, data, context)
    }
    fn offers(&self) -> &'static [&'static str] {
        (**self).offers()
    }
    fn raw(&self, data: &Envelope, context: Context) -> V {
        (**self).raw(data, context)
    }
    fn skeleton(&self, kind: Kind, context: Context) -> V {
        (**self).skeleton(kind, context)
    }
    fn placeholder(&self, kind: Kind, context: Context) -> V {
        (**self).placeholder(kind, context)
    }
    fn stylesheet(&self) -> &'static str {
        (**self).stylesheet()
    }
}
