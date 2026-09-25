//! A widget, described in a few lines, and the manifest built from it.
//!
//! Every widget is a platform with one panel, drawn by its own module. What
//! differs between them is a name, a title, whether people share what it
//! shows, and whether the shell has a sensible way to draw it without the
//! module. Everything else in the manifest is the same for all twenty, and
//! lives here once:
//!
//! | Field | Always |
//! |---|---|
//! | `assets` | `/ui/` |
//! | `ui.entry` | `/ui/{panel}/index.html` |
//! | `routes` | read and write under `/api/` |
//! | `events` | `api/events` |
//! | `health` | `api/health` |
//! | fallback `data` | `api/panels/{panel}`, where there is a fallback |
//! | `params` | `time_range` where the fallback is a series, none otherwise |
//!
//! One prefix for reads and writes, because a widget authorizes every request
//! itself and needs no protecting from its own module; what is outside it (the
//! manifest) no page can reach through the shell at all.

use hlin_identity::Claims;
use hlin_manifest::envelope::Envelope;
use hlin_manifest::manifest::{
    Lifecycle, Manifest, ModuleUi, Panel, ParamDecl, Platform, Routes, SUPPORTED_SCHEMA_VERSION,
};

/// The contract version every widget declares.
///
/// 1.1.0 because a widget ships a module (`assets` and `ui`), which is what
/// 1.1.0 added for the sample platforms.
pub const CONTRACT_VERSION: &str = "1.1.0";

/// Where every widget's module assets live.
pub const ASSETS: &str = "/ui/";

/// The prefix a widget's module may read and write under.
pub const API_PREFIX: &str = "/api/";

/// Where every widget's event stream is.
pub const EVENTS: &str = "api/events";

/// Where every widget answers health.
pub const HEALTH: &str = "api/health";

/// One widget: everything about it the shell is told.
///
/// `S` is the widget's own state, which only the fallback needs to see.
pub struct Widget<S> {
    /// The panel's key. Layouts name the panel `{platform}/{panel}`.
    pub panel: &'static str,
    /// The platform's display name, as the panel picker shows it.
    pub name: &'static str,
    /// An icon name, from the shell's vocabulary.
    pub icon: &'static str,
    /// The panel's default title.
    pub title: &'static str,
    /// What the panel shows, for the panel picker.
    pub description: &'static str,
    /// Whether one person's change is something everybody sees.
    ///
    /// Declared as the panel's `pushed`: a shared widget announces every write
    /// on its stream, so the shell can stop polling its fallback and wait to
    /// be told. Every widget announces its writes regardless (the same person
    /// in two browsers wants both to agree); only a shared one promises it.
    pub shared: bool,
    /// How the shell draws the panel where the module cannot be loaded, if
    /// there is a sensible way. `None` for a widget with nothing to fall back
    /// to, which should say why in its own docs.
    pub fallback: Option<Fallback<S>>,
    /// Where Trunk puts this widget's built module, in this repository:
    /// `concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist")`, so it is the
    /// widget crate's directory and not this one's.
    pub built: &'static str,
}

/// A shell-drawn view of the panel, for when the module is not there.
///
/// A fallback whose envelope is `series.v1` is over time, and so over the
/// surface's time range: the panel declares `time_range`, which is also what
/// makes the shell show its time picker and send the range to the module in
/// `context`. `data` answers everything the widget has, and the fallback route
/// keeps the points between the `from` and `to` the shell asked for.
pub struct Fallback<S> {
    /// The view kind: `stat`, `table`, `chart`, `status`.
    pub kind: &'static str,
    /// The envelope `data` answers with: `scalar.v1`, `records.v1`, ...
    pub envelope: &'static str,
    /// How often the fallback is worth refetching, where it changes without
    /// a write (a clock). `None` leaves it to the shell.
    pub refresh_ms: Option<u64>,
    /// The fallback's data, as this person would see it.
    pub data: fn(&S, &Claims) -> Envelope,
}

impl<S> Widget<S> {
    /// Where the panel's module entry is.
    pub fn entry(&self) -> String {
        format!("{ASSETS}{}/index.html", self.panel)
    }

    /// Where the module's files are served from.
    pub fn module_dir(&self) -> String {
        format!("{ASSETS}{}/", self.panel)
    }

    /// Where the fallback's data is, relative to the platform's base.
    pub fn fallback_data(&self) -> String {
        format!("api/panels/{}", self.panel)
    }

    /// Whether the panel follows the surface's time range: whether its
    /// fallback is a series (see [`Fallback`]).
    pub fn over_time(&self) -> bool {
        self.fallback
            .as_ref()
            .is_some_and(|fallback| fallback.envelope == hlin_manifest::envelope::SERIES_V1)
    }

    /// The manifest for this widget, served as the platform called `id`.
    pub fn manifest(&self, id: &str) -> Manifest {
        let fallback = self.fallback.as_ref();
        Manifest {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            contract_version: CONTRACT_VERSION.parse().expect("a valid semver constant"),
            platform: Platform {
                id: id.to_string(),
                name: self.name.to_string(),
                icon: Some(self.icon.to_string()),
                extra: Default::default(),
            },
            navigation: vec![],
            panels: vec![Panel {
                key: self.panel.to_string(),
                title: self.title.to_string(),
                description: Some(self.description.to_string()),
                kind: fallback.map(|fallback| fallback.kind.to_string()),
                ui: Some(ModuleUi {
                    entry: self.entry(),
                    bridge: 1,
                    extra: Default::default(),
                }),
                envelope: fallback.map(|fallback| fallback.envelope.to_string()),
                data: fallback.map(|_| self.fallback_data()),
                params: if self.over_time() {
                    vec![ParamDecl::bare(hlin_manifest::params::TIME_RANGE)]
                } else {
                    vec![]
                },
                refresh_ms: fallback.and_then(|fallback| fallback.refresh_ms),
                pushed: self.shared,
                component: None,
                lifecycle: Lifecycle::default(),
                extra: Default::default(),
            }],
            health: HEALTH.to_string(),
            events: Some(EVENTS.to_string()),
            assets: Some(ASSETS.to_string()),
            routes: Some(Routes {
                read: vec![API_PREFIX.to_string()],
                write: vec![API_PREFIX.to_string()],
                extra: Default::default(),
            }),
            extra: Default::default(),
        }
    }

    /// Why the shell would refuse or partly ignore this widget's manifest, if
    /// it would.
    ///
    /// A reference that is wrong is worse than none, so [`crate::serve()`]
    /// refuses to start a widget this says anything about, and every widget's
    /// tests call it.
    pub fn defects(&self, id: &str) -> Option<String> {
        let document = self.manifest(id);
        let checked = hlin_manifest::validate(&document, id);
        if let Some(defect) = &checked.document {
            return Some(format!("the manifest is malformed: {defect}"));
        }
        if !checked.rejected().is_empty() {
            return Some(format!("the shell rejects {:?}", checked.rejected()));
        }
        if !checked.unusable_routes.is_empty() {
            return Some(format!("unusable routes: {:?}", checked.unusable_routes));
        }
        if let Some(events) = &checked.unusable_events {
            return Some(format!("unusable events: {events:?}"));
        }
        if let Some(assets) = &checked.unusable_assets {
            return Some(format!("unusable assets: {assets:?}"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hlin_manifest::envelope::Scalar;

    fn widget(fallback: bool) -> Widget<u32> {
        Widget {
            panel: "count",
            name: "Counter",
            icon: "hash",
            title: "Counter",
            description: "A number",
            shared: true,
            fallback: fallback.then_some(Fallback {
                kind: "stat",
                envelope: "scalar.v1",
                refresh_ms: None,
                data: |count, _| {
                    Envelope::Scalar(Scalar {
                        value: (*count).into(),
                        unit: None,
                        label: None,
                        previous: None,
                        as_of: None,
                        extra: Default::default(),
                    })
                },
            }),
            built: "/nowhere",
        }
    }

    #[test]
    fn a_widget_with_a_fallback_is_a_manifest_the_shell_accepts_whole() {
        assert_eq!(widget(true).defects("counter"), None);
        let document = widget(true).manifest("counter");
        let panel = document.panel("count").unwrap();
        assert_eq!(panel.kind.as_deref(), Some("stat"));
        assert_eq!(panel.data.as_deref(), Some("api/panels/count"));
        assert_eq!(panel.ui.as_ref().unwrap().entry, "/ui/count/index.html");
        assert!(panel.pushed);
    }

    #[test]
    fn a_widget_without_a_fallback_is_a_module_and_nothing_else() {
        assert_eq!(widget(false).defects("counter"), None);
        let document = widget(false).manifest("counter");
        let panel = document.panel("count").unwrap();
        assert_eq!(panel.kind, None);
        assert_eq!(panel.data, None);
        assert!(panel.ui.is_some());
    }

    #[test]
    fn the_entry_is_under_the_assets_prefix_the_shell_serves() {
        let widget = widget(true);
        assert!(hlin_manifest::path::falls_under(ASSETS, &widget.entry()));
    }
}
