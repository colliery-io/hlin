//! The shell a person sees at `/`: a surface of shell components (decision
//! HLIN-A-0015, D13).
//!
//! An operator names one in `hlin.toml` as `default_surface`, a JSON file read
//! and checked at startup. Without one, the shell draws its own, for whoever
//! is looking: a bar across the top, a rail of pages down the side — their own
//! surfaces, and each platform's pages — and a content component, `main`,
//! that opens their home surface first.
//!
//! The shell is not stored and not edited. It holds shell components only: a
//! panel needs a stored surface to stream into, so panels go on a surface that
//! the shell opens.

use std::collections::BTreeSet;
use std::path::Path;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use hlin_stream::layout::{
    BarItem, ContentItem, LayoutDocument, LayoutItem, Page, PageEntry, PageGroup, PageSelectorItem,
    Placement, PlatformPage, SelectorStyle,
};
use serde::Deserialize;

use crate::identity::Caller;
use crate::layouts::Refusal;
use crate::server::AppState;

/// What `/api/shell` calls the shell, as the document's visibility: it is not
/// a layout anybody stored.
pub const SHELL_VISIBILITY: &str = "shell";

/// The content component the built-in shell opens pages in.
pub const MAIN: &str = "main";

/// An operator's shell file: a title, and its items.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellFile {
    #[serde(default)]
    title: Option<String>,
    items: Vec<serde_json::Value>,
}

/// Read and check the operator's shell, from the file `default_surface` names.
///
/// The reason names the file and what is wrong with it, because it is read at
/// startup, where an operator is watching, and nowhere else would say it.
pub fn read_operator_shell(path: &Path) -> Result<LayoutDocument, String> {
    let shown = path.display();
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("default_surface {shown} could not be read: {error}"))?;
    let file: ShellFile = serde_json::from_str(&text)
        .map_err(|error| format!("default_surface {shown} is not a shell: {error}"))?;

    let mut items = Vec::with_capacity(file.items.len());
    let mut ids = BTreeSet::new();
    for (index, item) in file.items.into_iter().enumerate() {
        let kind = item
            .get("type")
            .and_then(|kind| kind.as_str())
            .unwrap_or_default()
            .to_string();
        if !LayoutItem::TYPES.contains(&kind.as_str()) {
            return Err(format!(
                "default_surface {shown}: item {index} has type `{kind}`. Known types: {}",
                LayoutItem::TYPES.join(", ")
            ));
        }
        let item: LayoutItem = serde_json::from_value(item)
            .map_err(|error| format!("default_surface {shown}: item {index}: {error}"))?;
        if item.as_panel().is_some() {
            return Err(format!(
                "default_surface {shown}: item {index} is a panel. A shell holds shell \
                 components only; put panels on a surface, and open it in a content component"
            ));
        }
        let Some(id) = item.id().map(str::trim).filter(|id| !id.is_empty()) else {
            return Err(format!(
                "default_surface {shown}: item {index} has no `id`. A selector names its \
                 content component by id"
            ));
        };
        if !ids.insert(id.to_string()) {
            return Err(format!(
                "default_surface {shown}: two items have the id `{id}`"
            ));
        }
        items.push(item);
    }

    let mut document = LayoutDocument::empty(file.title.as_deref().unwrap_or("Shell"));
    document.visibility = SHELL_VISIBILITY.to_string();
    document.editable = false;
    document.items = items;
    Ok(document)
}

/// The shell for whoever is asking.
pub async fn shell(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Result<Json<LayoutDocument>, Refusal> {
    if let Some(path) = &state.config.default_surface {
        // Checked at startup. A file changed or removed since is the
        // operator's to put right, and the reason says which.
        return read_operator_shell(path)
            .map(Json)
            .map_err(|reason| Refusal::saying(StatusCode::INTERNAL_SERVER_ERROR, reason));
    }
    Ok(Json(built_in(&state, principal).await))
}

/// The shell Hlin draws when the operator names none.
async fn built_in(state: &AppState, principal: hlin_identity::Principal) -> LayoutDocument {
    // The surface `main` opens first: the same one `/api/layouts/home` names.
    let home = crate::layouts::home(State(state.clone()), Caller(principal.clone()))
        .await
        .ok()
        .and_then(|Json(document)| document.id);

    let mut groups = Vec::new();

    // A read-only shell owns nothing for anybody.
    if !state.config.read_only() {
        let owned = state
            .store
            .layouts_owned_by(&principal.sub)
            .await
            .unwrap_or_default();
        let surfaces: Vec<PageEntry> = owned
            .iter()
            .map(|layout| PageEntry {
                page: Page::Surface(layout.id.to_string()),
                label: None,
            })
            .collect();
        if !surfaces.is_empty() {
            groups.push(PageGroup {
                label: "Surfaces".to_string(),
                pages: surfaces,
            });
        }
    }

    // Each platform's pages: the navigation entries it draws with a module, in
    // its own order.
    for view in state.registry.views().await.values() {
        let mut entries: Vec<_> = view
            .accepted_navigation()
            .into_iter()
            .filter(|entry| entry.ui.is_some())
            .collect();
        entries.sort_by(|left, right| {
            left.weight
                .cmp(&right.weight)
                .then_with(|| left.label.cmp(&right.label))
        });
        if entries.is_empty() {
            continue;
        }
        let name = view.manifest.as_ref().map_or_else(
            || view.config.id.clone(),
            |manifest| manifest.platform.name.clone(),
        );
        groups.push(PageGroup {
            label: name,
            pages: entries
                .into_iter()
                .map(|entry| PageEntry {
                    page: Page::Page(PlatformPage {
                        platform: view.config.id.clone(),
                        path: entry.path,
                    }),
                    label: Some(entry.label),
                })
                .collect(),
        });
    }

    let mut document = LayoutDocument::empty("Shell");
    document.visibility = SHELL_VISIBILITY.to_string();
    document.editable = false;
    document.items = vec![
        LayoutItem::Bar(BarItem {
            id: Some("bar".to_string()),
            position: Placement {
                x: 0,
                y: 0,
                w: 12,
                h: 1,
            },
        }),
        LayoutItem::PageSelector(PageSelectorItem {
            id: Some("pages".to_string()),
            position: Placement {
                x: 0,
                y: 1,
                w: 2,
                h: 12,
            },
            style: SelectorStyle::Rail,
            target: MAIN.to_string(),
            groups,
        }),
        LayoutItem::Content(ContentItem {
            id: Some(MAIN.to_string()),
            position: Placement {
                x: 2,
                y: 1,
                w: 10,
                h: 12,
            },
            initial: home.map(Page::Surface),
        }),
    ];
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file holding `text`, in the temporary directory, named for the test.
    fn written(text: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "hlin-shell-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, text).expect("write");
        path
    }

    #[test]
    fn an_operators_shell_is_read_as_a_surface_nobody_edits() {
        let file = written(
            r#"{ "title": "Ops", "items": [
                { "type": "bar", "id": "bar", "position": { "x": 0, "y": 0, "w": 12, "h": 1 } },
                { "type": "page-selector", "id": "nav", "target": "main", "style": "tabs",
                  "position": { "x": 0, "y": 1, "w": 12, "h": 1 },
                  "groups": [ { "label": "Views", "pages": [ { "view": "about" } ] } ] },
                { "type": "content", "id": "main", "position": { "x": 0, "y": 2, "w": 12, "h": 8 },
                  "initial": { "view": "about" } }
            ] }"#,
        );
        let shell = read_operator_shell(&file).expect("a valid shell");
        assert_eq!(shell.title, "Ops");
        assert_eq!(shell.visibility, SHELL_VISIBILITY);
        assert!(!shell.editable);
        assert_eq!(shell.id, None);
        assert_eq!(shell.items.len(), 3);
    }

    #[test]
    fn a_missing_file_says_which() {
        let reason = read_operator_shell(Path::new("/tmp/hlin-no-such-shell.json")).unwrap_err();
        assert!(reason.contains("/tmp/hlin-no-such-shell.json"), "{reason}");
        assert!(reason.contains("could not be read"), "{reason}");
    }

    #[test]
    fn what_is_not_a_shell_is_refused_with_a_reason() {
        for (text, says) in [
            ("not json", "is not a shell"),
            (
                r#"{ "items": [ { "type": "widget", "id": "w" } ] }"#,
                "`widget`",
            ),
            (r#"{ "items": [ { "id": "w" } ] }"#, "type ``"),
            (
                r#"{ "items": [ { "type": "panel", "id": "p", "platform_id": "a", "panel_key": "b" } ] }"#,
                "is a panel",
            ),
            (r#"{ "items": [ { "type": "bar" } ] }"#, "has no `id`"),
            (
                r#"{ "items": [ { "type": "bar", "id": "x" }, { "type": "content", "id": "x" } ] }"#,
                "two items have the id `x`",
            ),
            (r#"{ "items": [], "panels": [] }"#, "is not a shell"),
        ] {
            let file = written(text);
            let reason = read_operator_shell(&file).unwrap_err();
            assert!(reason.contains(says), "{text}: {reason}");
        }
    }
}
