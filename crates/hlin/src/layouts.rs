//! The layout API: what a person composed, and what they may compose from.
//!
//! Composition is the claim the product exists to make, and this is its whole
//! write path. Two rules from decision HLIN-A-0007 are enforced here and
//! nowhere else, so there is one place to read them:
//!
//! - **One owner.** A layout belongs to the principal who created it. Anyone
//!   who can see a layout can read it; only its owner can write it. Everyone
//!   else forks.
//! - **Whole-layout replacement.** A `PUT` carries the entire layout, because
//!   a layout is edited as one thing. Dragging one panel moves several, and
//!   there is no useful meaning to half of that having happened.
//!
//! The catalogue the picker reads is here too, because it is the read side of
//! the same question: a layout names panels, and this is the list of panels
//! there are to name.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hlin_stream::layout::{
    CatalogPanel, CatalogPlatform, LayoutDocument, LayoutSummary, PanelInstanceDocument, Placement,
};
use uuid::Uuid;

use crate::identity::Caller;
use crate::server::AppState;
use crate::store::{Layout, NewLayout, PanelInstance, Visibility};

/// What a layout request could not do, and what the browser should be told.
///
/// Small and typed rather than a built response, so a handler cannot answer a
/// permissions question with a status that contradicts it.
#[derive(Debug)]
pub struct Refusal {
    status: StatusCode,
    reason: String,
}

impl Refusal {
    /// A refusal with a status and a reason, for callers outside this module.
    pub fn saying(status: StatusCode, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }

    fn not_found(what: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            reason: format!("no such {what}"),
        }
    }

    fn forbidden(reason: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            reason: reason.to_string(),
        }
    }

    fn unusable(reason: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            reason,
        }
    }

    fn store(error: crate::store::StoreError) -> Self {
        // A store failure is an operational problem, not something the viewer
        // did, and saying so plainly beats a 400 that invites them to retype
        // a title.
        tracing::error!(%error, "the store failed during a layout operation");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            reason: "the shell could not reach its store".to_string(),
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.reason })),
        )
            .into_response()
    }
}

type Answer<T> = Result<Json<T>, Refusal>;

fn identifier(raw: &str) -> Result<Uuid, Refusal> {
    Uuid::parse_str(raw).map_err(|_| Refusal::not_found("layout"))
}

// -- The catalogue ---------------------------------------------------------

/// Every panel a viewer could put on a surface, grouped by platform.
///
/// Accepted panels only: a panel the registry rejected is a defect an operator
/// needs to see, not a choice to offer someone composing a surface. Those are
/// reported by `/api/platforms` instead.
pub async fn catalog(State(state): State<AppState>) -> Json<Vec<CatalogPlatform>> {
    let views = state.registry.views().await;

    let platforms = views
        .values()
        .map(|view| CatalogPlatform {
            id: view.config.id.clone(),
            name: view.manifest.as_ref().map(|m| m.platform.name.clone()),
            reachable: view.reachable,
            panels: view
                .accepted_panels()
                .into_iter()
                .map(|panel| catalog_panel(&view.config.id, panel))
                .collect(),
        })
        .collect();

    Json(platforms)
}

/// One declared panel, as both the picker and `/api/platforms` describe it.
///
/// One function rather than two similar ones, so an operator's view of a panel
/// and a composer's view of it cannot drift apart.
pub fn catalog_panel(platform_id: &str, panel: &hlin_manifest::Panel) -> CatalogPanel {
    CatalogPanel {
        key: panel.key.clone(),
        reference: format!("{platform_id}/{}", panel.key),
        title: panel.title.clone(),
        description: panel.description.clone(),
        kind: panel.kind.clone(),
        refresh_ms: panel.refresh_ms,
        component: panel.component.clone(),
        pushed: panel.pushed,
        envelope: panel.envelope.clone(),
        params: panel
            .params
            .iter()
            .map(|declaration| declaration.param.clone())
            .collect(),
        controls: panel.params.iter().filter_map(control_for).collect(),
        available_kinds: hlin_view::Kind::accepting(&panel.envelope)
            .into_iter()
            .map(|kind| kind.name().to_string())
            .collect(),
        deprecated: panel.lifecycle.is_deprecated(),
        successor: panel.lifecycle.successor.clone(),
    }
}

/// A parameter declaration, as a control the viewer can set.
///
/// Only declarations naming a query key of their own become controls. A
/// `time_range` is driven by the surface's one picker and is not a per-panel
/// choice, and a declaration with no `id` names no key to send a value under,
/// so there is nothing a control could do with it.
fn control_for(
    declaration: &hlin_manifest::ParamDecl,
) -> Option<hlin_stream::layout::PanelControl> {
    let id = declaration.config.get("id")?.as_str()?.to_string();

    Some(hlin_stream::layout::PanelControl {
        param: declaration.param.clone(),
        label: declaration
            .config
            .get("label")
            .and_then(|label| label.as_str())
            .unwrap_or(&id)
            .to_string(),
        options: declaration
            .config
            .get("options")
            .and_then(|options| options.as_str())
            .map(str::to_string),
        id,
    })
}

// -- Layouts ---------------------------------------------------------------

/// The principal's own layouts, most recently changed first.
pub async fn list(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Answer<Vec<LayoutSummary>> {
    let owner = principal.sub.clone();
    let layouts = state
        .store
        .layouts_owned_by(&owner)
        .await
        .map_err(Refusal::store)?;

    Ok(Json(layouts.iter().map(summarise).collect()))
}

/// What to open when nobody has said which surface they want.
///
/// A first visit should land on something a person can compose, not on a list
/// with nothing in it. This returns the principal's most recently changed
/// layout, creating an empty one the first time. Making the shell do it means
/// the browser has no "if there are none" branch to get wrong, and two tabs
/// opening at once cannot each create a layout by racing.
pub async fn home(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Answer<LayoutDocument> {
    let existing = state
        .store
        .layouts_owned_by(&principal.sub)
        .await
        .map_err(Refusal::store)?;

    if let Some(layout) = existing.into_iter().next() {
        return Ok(Json(document_for(&layout, &principal.sub)));
    }

    let created = state
        .store
        .create_layout(NewLayout::personal(&principal.sub, "My surface"))
        .await
        .map_err(Refusal::store)?;

    tracing::info!(
        owner = principal.sub,
        layout = %created.id,
        "created a first layout"
    );
    Ok(Json(document_for(&created, &principal.sub)))
}

/// What a browser sends to start a new surface.
#[derive(Debug, serde::Deserialize)]
pub struct NewLayoutRequest {
    /// What to call it.
    #[serde(default)]
    pub title: Option<String>,
}

/// Start a new, empty surface.
pub async fn create(
    State(state): State<AppState>,
    Caller(principal): Caller,
    Json(request): Json<NewLayoutRequest>,
) -> Result<(StatusCode, Json<LayoutDocument>), Refusal> {
    let owner = principal.sub.clone();
    let title = request
        .title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "New surface".to_string());

    let created = state
        .store
        .create_layout(NewLayout::personal(&owner, title))
        .await
        .map_err(Refusal::store)?;

    Ok((StatusCode::CREATED, Json(document_for(&created, &owner))))
}

/// One layout.
pub async fn read(
    State(state): State<AppState>,
    Caller(principal): Caller,
    Path(id): Path<String>,
) -> Answer<LayoutDocument> {
    let principal = principal.sub.clone();
    let layout = load_visible(&state, &id, &principal).await?;
    Ok(Json(document_for(&layout, &principal)))
}

/// Replace a layout.
///
/// The whole document, per the store's contract. What the browser sends is
/// taken as the arrangement in full: panels it omitted are gone, and panels it
/// added arrive without identifiers, which are assigned here.
pub async fn replace(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Path(id): Path<String>,
    Json(document): Json<LayoutDocument>,
) -> Answer<LayoutDocument> {
    let principal = caller.sub.clone();
    let uuid = identifier(&id)?;

    let mut stored = state
        .store
        .layout(uuid)
        .await
        .map_err(Refusal::store)?
        .ok_or_else(|| Refusal::not_found("layout"))?;

    if !stored.is_editable_by(&principal) {
        // Not a 404: hiding the layout's existence from someone who can see it
        // would make the fork path incomprehensible. They may read it; they may
        // not write it.
        return Err(Refusal::forbidden(
            "this layout belongs to someone else. Fork it to make changes",
        ));
    }

    let title = document.title.trim();
    if title.is_empty() {
        return Err(Refusal::unusable("a layout needs a title".to_string()));
    }
    stored.title = title.to_string();

    if let Some(visibility) = Visibility::parse(&document.visibility) {
        stored.visibility = visibility;
    }

    stored.panels = document
        .panels
        .iter()
        .map(|panel| instance_from(panel, &stored))
        .collect::<Result<Vec<_>, _>>()?;

    state
        .store
        .update_layout(&stored)
        .await
        .map_err(Refusal::store)?;

    // The surfaces already running for this layout were built from the panels
    // it used to have. Dropping them means the next subscription rebuilds from
    // what was just written, which is why the browser re-subscribes after a
    // write lands rather than hoping the stream catches up.
    // Reconciled rather than dropped. A surface holds what its panels last
    // fetched, and a write that moved one panel should not cost every other
    // panel its data.
    let reconciled = state.surfaces.reconcile(&id, &caller, &state).await;
    tracing::debug!(layout = %id, reconciled, "layout replaced; surfaces reconciled");

    Ok(Json(document_for(&stored, &principal)))
}

/// Delete a layout.
pub async fn remove(
    State(state): State<AppState>,
    Caller(principal): Caller,
    Path(id): Path<String>,
) -> Result<StatusCode, Refusal> {
    let principal = principal.sub.clone();
    let uuid = identifier(&id)?;

    let stored = state
        .store
        .layout(uuid)
        .await
        .map_err(Refusal::store)?
        .ok_or_else(|| Refusal::not_found("layout"))?;

    if !stored.is_editable_by(&principal) {
        return Err(Refusal::forbidden("this layout belongs to someone else"));
    }

    state
        .store
        .delete_layout(uuid)
        .await
        .map_err(Refusal::store)?;
    state.surfaces.forget(&id).await;

    Ok(StatusCode::NO_CONTENT)
}

/// Copy someone else's layout so it can be changed.
///
/// The whole of the sharing model's write path: a viewer who wants to edit a
/// layout they do not own gets their own copy, with a record of where it came
/// from (decision HLIN-A-0007).
pub async fn fork(
    State(state): State<AppState>,
    Caller(principal): Caller,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<LayoutDocument>), Refusal> {
    let principal = principal.sub.clone();
    let uuid = identifier(&id)?;

    let source = load_visible(&state, &id, &principal).await?;
    let _ = source;

    let forked = state
        .store
        .fork_layout(uuid, &principal)
        .await
        .map_err(Refusal::store)?;

    Ok((StatusCode::CREATED, Json(document_for(&forked, &principal))))
}

async fn load_visible(state: &AppState, id: &str, principal: &str) -> Result<Layout, Refusal> {
    let uuid = identifier(id)?;
    let layout = state
        .store
        .layout(uuid)
        .await
        .map_err(Refusal::store)?
        .ok_or_else(|| Refusal::not_found("layout"))?;

    if !layout.is_visible_to(principal) {
        // Here a 404 is right: a personal layout's existence is not this
        // principal's business, and knowing the link is the permission.
        return Err(Refusal::not_found("layout"));
    }

    Ok(layout)
}

// -- Converting at the edge ------------------------------------------------

fn summarise(layout: &Layout) -> LayoutSummary {
    LayoutSummary {
        id: layout.id.to_string(),
        title: layout.title.clone(),
        visibility: layout.visibility.as_str().to_string(),
        owner: layout.owner.clone(),
        panel_count: layout.panels.len(),
    }
}

/// A stored layout, as the browser reads it.
pub fn document_for(layout: &Layout, principal: &str) -> LayoutDocument {
    LayoutDocument {
        id: Some(layout.id.to_string()),
        title: layout.title.clone(),
        visibility: layout.visibility.as_str().to_string(),
        owner: layout.owner.clone(),
        editable: layout.is_editable_by(principal),
        panels: layout.panels.iter().map(instance_document).collect(),
    }
}

fn instance_document(instance: &PanelInstance) -> PanelInstanceDocument {
    PanelInstanceDocument {
        id: Some(instance.id.to_string()),
        platform_id: instance.platform_id.clone(),
        panel_key: instance.panel_key.clone(),
        kind_override: instance.kind_override.clone(),
        title_override: instance.title_override.clone(),
        selections: serde_json::from_value(instance.selections.clone()).unwrap_or_default(),
        position: placement_of(instance),
    }
}

/// Where a stored panel sits.
///
/// `position` is free-form JSON in the store, so that the grid can change shape
/// without a migration. A position this shell cannot read is a default rather
/// than an error: losing an arrangement is bad, and refusing to draw the panel
/// at all is worse.
pub fn placement_of(instance: &PanelInstance) -> Placement {
    serde_json::from_value::<Placement>(instance.position.clone())
        .unwrap_or_default()
        .clamped()
}

fn instance_from(
    document: &PanelInstanceDocument,
    layout: &Layout,
) -> Result<PanelInstance, Refusal> {
    let position = serde_json::to_value(document.position.clamped())
        .map_err(|error| Refusal::unusable(error.to_string()))?;
    let selections = serde_json::to_value(&document.selections)
        .map_err(|error| Refusal::unusable(error.to_string()))?;

    // A panel that was already on this layout keeps its identity, so the
    // browser's open stream keeps naming the same thing. One the browser
    // invented an id for is not honoured: identity is assigned here.
    let existing = document
        .id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .and_then(|id| layout.panels.iter().find(|panel| panel.id == id));

    let mut instance = match existing {
        Some(panel) => panel.clone(),
        None => PanelInstance::new(
            document.platform_id.clone(),
            document.panel_key.clone(),
            position.clone(),
        ),
    };

    instance.platform_id = document.platform_id.clone();
    instance.panel_key = document.panel_key.clone();
    instance.kind_override = document.kind_override.clone();
    instance.title_override = document.title_override.clone();
    instance.selections = selections;
    instance.position = position;

    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn layout_with(panels: Vec<PanelInstance>) -> Layout {
        Layout {
            id: Uuid::new_v4(),
            owner: "ada".to_string(),
            title: "Mine".to_string(),
            visibility: Visibility::Personal,
            forked_from: None,
            time_range: None,
            panels,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn a_panel_already_on_the_layout_keeps_its_identity() {
        let existing = PanelInstance::new("orders", "queue-depth", serde_json::json!({}));
        let id = existing.id;
        let layout = layout_with(vec![existing]);

        let document = PanelInstanceDocument {
            id: Some(id.to_string()),
            platform_id: "orders".to_string(),
            panel_key: "queue-depth".to_string(),
            kind_override: Some("gauge".to_string()),
            title_override: None,
            selections: Default::default(),
            position: Placement {
                x: 4,
                y: 2,
                w: 4,
                h: 3,
            },
        };

        let rebuilt = instance_from(&document, &layout).expect("a valid instance");
        assert_eq!(rebuilt.id, id, "the stream keeps naming the same panel");
        assert_eq!(rebuilt.kind_override.as_deref(), Some("gauge"));
    }

    #[test]
    fn an_identifier_the_browser_invented_is_not_honoured() {
        let layout = layout_with(vec![]);
        let invented = Uuid::new_v4();

        let document = PanelInstanceDocument {
            id: Some(invented.to_string()),
            platform_id: "orders".to_string(),
            panel_key: "queue-depth".to_string(),
            kind_override: None,
            title_override: None,
            selections: Default::default(),
            position: Placement::default(),
        };

        let rebuilt = instance_from(&document, &layout).expect("a valid instance");
        assert_ne!(
            rebuilt.id, invented,
            "identity is the store's to assign, or a browser could collide two panels"
        );
    }

    #[test]
    fn an_unreadable_position_is_a_default_rather_than_a_refusal() {
        let instance =
            PanelInstance::new("orders", "queue-depth", serde_json::json!("bottom left"));
        let placement = placement_of(&instance);
        assert_eq!(placement, Placement::default().clamped());
    }

    #[test]
    fn a_document_says_whether_the_asker_may_write_it() {
        let layout = layout_with(vec![]);
        assert!(document_for(&layout, "ada").editable);
        assert!(
            !document_for(&layout, "grace").editable,
            "everyone else forks to edit"
        );
    }
}
