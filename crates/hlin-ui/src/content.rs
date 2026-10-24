//! What a content component opens (decision HLIN-A-0015).
//!
//! A content component shows one page in its cells: a surface, a platform's
//! module page, or a view the front end registered in code. Which one, and
//! whether it may be opened at all, is decided here, free of the DOM and tested
//! on the host like [`crate::route`]: a surface that opens itself, or opens
//! surfaces without end, is the kind of mistake a browser shows as a page that
//! never finishes loading.

use std::collections::BTreeSet;
use std::fmt;

use hlin_stream::layout::{CatalogPlatform, Page};

use crate::route::{self, PageTarget};

/// How deep a surface may be: the outermost is 0, and a surface opened in a
/// content component is one deeper than the surface holding it.
pub const MAX_DEPTH: usize = 2;

/// What a content component draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// A surface, by its id, one level deeper.
    Surface(String),
    /// A platform's module page.
    Platform(PageTarget),
    /// A view the front end registered, by its id.
    View(String),
    /// The catalogue has not answered yet, so a platform page cannot be told
    /// apart from one that does not exist.
    Waiting,
    /// Nothing that can be drawn, and why.
    Missing(Missing),
}

/// Why a content component shows a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// No page is chosen and none is set to show first.
    NothingChosen,
    /// The surface would be deeper than [`MAX_DEPTH`].
    TooDeep,
    /// The surface is already open around this one.
    Cycle(String),
    /// No platform offers this page with a module.
    NoSuchPage(PageTarget),
    /// The front end registered no view with this id.
    NoSuchView(String),
}

impl Missing {
    /// One word for the markup, so a test can say which placeholder it sees.
    pub fn cause(&self) -> &'static str {
        match self {
            Self::NothingChosen => "nothing-chosen",
            Self::TooDeep => "too-deep",
            Self::Cycle(_) => "cycle",
            Self::NoSuchPage(_) => "no-such-page",
            Self::NoSuchView(_) => "no-such-view",
        }
    }
}

impl fmt::Display for Missing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingChosen => write!(formatter, "No page is chosen."),
            Self::TooDeep => write!(
                formatter,
                "This surface is not shown here. Surfaces open at most {MAX_DEPTH} levels deep."
            ),
            Self::Cycle(id) => write!(
                formatter,
                "The surface {id} is not shown here, because it is already open around this one."
            ),
            Self::NoSuchPage(target) => write!(
                formatter,
                "The platform {} offers no page {}.",
                target.platform, target.path
            ),
            Self::NoSuchView(id) => write!(formatter, "This front end has no view {id}."),
        }
    }
}

/// What a content component at `depth` opens for `page`.
///
/// `depth` is the depth of the surface holding the component, and `ancestry`
/// the ids of that surface and every surface around it.
pub fn open(
    page: Option<&Page>,
    depth: usize,
    ancestry: &[String],
    catalog: &[CatalogPlatform],
    catalog_known: bool,
    views: &BTreeSet<String>,
) -> Opened {
    let Some(page) = page else {
        return Opened::Missing(Missing::NothingChosen);
    };
    match page {
        Page::Surface(id) => {
            if ancestry.iter().any(|open| open == id) {
                Opened::Missing(Missing::Cycle(id.clone()))
            } else if depth + 1 > MAX_DEPTH {
                Opened::Missing(Missing::TooDeep)
            } else {
                Opened::Surface(id.clone())
            }
        }
        Page::Page(platform_page) => {
            let target = PageTarget {
                platform: platform_page.platform.clone(),
                path: platform_page.path.clone(),
            };
            if route::page_entry(catalog, &target).is_some() {
                Opened::Platform(target)
            } else if catalog_known {
                Opened::Missing(Missing::NoSuchPage(target))
            } else {
                Opened::Waiting
            }
        }
        Page::View(id) => {
            if views.contains(id) {
                Opened::View(id.clone())
            } else {
                Opened::Missing(Missing::NoSuchView(id.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hlin_stream::layout::{CatalogNavigation, CatalogUi, PlatformPage};

    fn catalog() -> Vec<CatalogPlatform> {
        vec![CatalogPlatform {
            id: "pipeline".into(),
            name: Some("Pipeline".into()),
            reachable: true,
            panels: vec![],
            module_limits: None,
            navigation: vec![
                CatalogNavigation {
                    label: "Runs".into(),
                    path: "runs".into(),
                    icon: None,
                    weight: 0,
                    ui: Some(CatalogUi {
                        entry: "/ui/index.html".into(),
                        bridge: 1,
                    }),
                },
                CatalogNavigation {
                    label: "Docs".into(),
                    path: "docs".into(),
                    icon: None,
                    weight: 0,
                    ui: None,
                },
            ],
        }]
    }

    fn views() -> BTreeSet<String> {
        ["audit-log".to_string()].into()
    }

    fn surface(id: &str) -> Page {
        Page::Surface(id.into())
    }

    fn page(platform: &str, path: &str) -> Page {
        Page::Page(PlatformPage {
            platform: platform.into(),
            path: path.into(),
        })
    }

    fn opened(page: Option<&Page>, depth: usize, ancestry: &[&str]) -> Opened {
        let ancestry: Vec<String> = ancestry.iter().map(|id| id.to_string()).collect();
        open(page, depth, &ancestry, &catalog(), true, &views())
    }

    #[test]
    fn nothing_chosen_is_a_placeholder() {
        assert_eq!(
            opened(None, 0, &["shell"]),
            Opened::Missing(Missing::NothingChosen)
        );
    }

    #[test]
    fn a_surface_opens_one_level_deeper_up_to_the_limit() {
        assert_eq!(
            opened(Some(&surface("ops")), 0, &["shell"]),
            Opened::Surface("ops".into())
        );
        assert_eq!(
            opened(Some(&surface("ingest")), 1, &["shell", "ops"]),
            Opened::Surface("ingest".into())
        );
    }

    #[test]
    fn a_surface_past_the_limit_is_too_deep() {
        assert_eq!(
            opened(Some(&surface("deeper")), 2, &["shell", "ops", "ingest"]),
            Opened::Missing(Missing::TooDeep)
        );
    }

    #[test]
    fn a_surface_open_around_this_one_is_a_cycle() {
        assert_eq!(
            opened(Some(&surface("shell")), 0, &["shell"]),
            Opened::Missing(Missing::Cycle("shell".into()))
        );
        assert_eq!(
            opened(Some(&surface("shell")), 1, &["shell", "ops"]),
            Opened::Missing(Missing::Cycle("shell".into()))
        );
    }

    #[test]
    fn a_cycle_is_named_before_depth() {
        // Both are true at the limit; the cycle is the more useful thing to say.
        assert_eq!(
            opened(Some(&surface("ops")), 2, &["shell", "ops", "ingest"]),
            Opened::Missing(Missing::Cycle("ops".into()))
        );
    }

    #[test]
    fn a_platform_page_with_a_module_opens() {
        assert_eq!(
            opened(Some(&page("pipeline", "runs")), 0, &[]),
            Opened::Platform(PageTarget {
                platform: "pipeline".into(),
                path: "runs".into(),
            })
        );
    }

    #[test]
    fn a_platform_page_nobody_offers_is_missing() {
        let target = |platform: &str, path: &str| PageTarget {
            platform: platform.into(),
            path: path.into(),
        };
        // An entry with no module is not a page the shell draws.
        assert_eq!(
            opened(Some(&page("pipeline", "docs")), 0, &[]),
            Opened::Missing(Missing::NoSuchPage(target("pipeline", "docs")))
        );
        assert_eq!(
            opened(Some(&page("nowhere", "runs")), 0, &[]),
            Opened::Missing(Missing::NoSuchPage(target("nowhere", "runs")))
        );
    }

    #[test]
    fn a_platform_page_waits_for_the_catalogue() {
        let unknown = page("pipeline", "runs");
        assert_eq!(
            open(Some(&unknown), 0, &[], &[], false, &views()),
            Opened::Waiting
        );
    }

    #[test]
    fn a_view_opens_only_if_the_front_end_has_it() {
        assert_eq!(
            opened(Some(&Page::View("audit-log".into())), 0, &[]),
            Opened::View("audit-log".into())
        );
        assert_eq!(
            opened(Some(&Page::View("gone".into())), 0, &[]),
            Opened::Missing(Missing::NoSuchView("gone".into()))
        );
    }

    #[test]
    fn every_placeholder_says_why() {
        for missing in [
            Missing::NothingChosen,
            Missing::TooDeep,
            Missing::Cycle("ops".into()),
            Missing::NoSuchPage(PageTarget {
                platform: "p".into(),
                path: "x".into(),
            }),
            Missing::NoSuchView("v".into()),
        ] {
            assert!(!missing.to_string().is_empty());
            assert!(!missing.cause().is_empty());
        }
    }
}
