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
//! The admin role (HLIN-A-0016) is checked here too: only an admin makes a
//! layout `public`, sets or clears the org default, or deletes a published or
//! public layout someone else owns.
//!
//! The catalogue the picker reads is here too, because it is the read side of
//! the same question: a layout names panels, and this is the list of panels
//! there are to name.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hlin_stream::layout::{
    CatalogPanel, CatalogPlatform, LayoutDocument, LayoutItem as ItemDocument, LayoutSummary,
    PanelInstanceDocument, Placement,
};
use uuid::Uuid;

use crate::identity::{Author, Caller, IsAdmin};
use crate::server::AppState;
use crate::store::{Layout, LayoutItem, NewLayout, PanelInstance, Visibility};

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
            module_limits: module_limits(&state.config, view),
            navigation: view
                .accepted_navigation()
                .into_iter()
                .map(|entry| hlin_stream::layout::CatalogNavigation {
                    label: entry.label,
                    path: entry.path,
                    icon: entry.icon,
                    weight: entry.weight,
                    ui: entry.ui.map(|ui| hlin_stream::layout::CatalogUi {
                        entry: ui.entry,
                        bridge: ui.bridge,
                    }),
                })
                .collect(),
        })
        .collect();

    Json(platforms)
}

/// One declared panel, as both the picker and `/api/platforms` describe it.
///
/// One function rather than two similar ones, so an operator's view of a panel
/// and a composer's view of it cannot drift apart.
///
/// A panel drawn only by its platform's module has no `kind`, `envelope` or
/// kinds to switch between: there is nothing for the shell to draw it with.
/// It is described all the same, because a person can put it on a surface and
/// the page mounts its module there.
pub fn catalog_panel(platform_id: &str, panel: &hlin_manifest::Panel) -> CatalogPanel {
    let drawn = panel.drawn_by_shell();
    CatalogPanel {
        key: panel.key.clone(),
        reference: format!("{platform_id}/{}", panel.key),
        title: panel.title.clone(),
        description: panel.description.clone(),
        kind: drawn.map(|drawn| drawn.kind.to_string()),
        ui: panel.ui.as_ref().map(|ui| hlin_stream::layout::CatalogUi {
            entry: ui.entry.clone(),
            bridge: ui.bridge,
        }),
        refresh_ms: panel.refresh_ms,
        component: panel.component.clone(),
        pushed: panel.pushed,
        envelope: drawn.map(|drawn| drawn.envelope.to_string()),
        params: panel
            .params
            .iter()
            .map(|declaration| declaration.param.clone())
            .collect(),
        controls: panel.params.iter().filter_map(control_for).collect(),
        available_kinds: drawn
            .map(|drawn| {
                hlin_view::Kind::accepting(drawn.envelope)
                    .into_iter()
                    .map(|kind| kind.name().to_string())
                    .collect()
            })
            .unwrap_or_default(),
        deprecated: panel.lifecycle.is_deprecated(),
        successor: panel.lifecycle.successor.clone(),
    }
}

/// The limits a platform's modules run within, as the page is told them.
///
/// Only for a platform whose manifest declares `assets`: without that it ships
/// no module, and telling the page limits for frames it will never mount is
/// noise in every catalogue answer.
pub fn module_limits(
    config: &crate::config::Config,
    view: &crate::registry::PlatformView,
) -> Option<hlin_stream::layout::ModuleLimits> {
    view.manifest.as_ref()?.assets.as_ref()?;
    let limits = config.module_limits(&view.config.id);
    Some(hlin_stream::layout::ModuleLimits {
        request_bytes: limits.request_bytes as u64,
        response_bytes: limits.response_bytes as u64,
        fetches_in_flight: limits.fetches_in_flight,
        messages_per_second: limits.messages_per_second,
        streams: limits.streams,
        state_bytes: limits.state_bytes as u64,
    })
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
/// Signed in: the principal's most recently changed layout; with none, the
/// org default, if one is set and still published or public; with neither,
/// an empty layout created for them. Making the shell decide means the
/// browser has no "if there are none" branch to get wrong, and two tabs
/// opening at once cannot each create a layout by racing.
///
/// Where nobody signs in (`anonymous`): the org default if it is public, else
/// the most recently changed public layout, else 404 with the reason. Never
/// a write (HLIN-A-0012, HLIN-A-0016).
pub async fn home(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Answer<LayoutDocument> {
    // Where nobody signs in there is nothing to own, so "your most recent
    // surface" has no meaning and creating one would be a write — an unbounded
    // one, a row per visitor, on a shell whose whole premise is that it stores
    // nothing they do. A visitor lands on what an admin made public instead.
    if state.config.read_only() {
        if let Some(layout) = org_default(&state, &principal.sub).await? {
            return Ok(Json(document_for(&layout, &principal.sub)));
        }

        let public = state.store.public_layouts().await.map_err(Refusal::store)?;

        return match public.into_iter().next() {
            Some(layout) => Ok(Json(document_for(&layout, &principal.sub))),
            // Not an error in the shell. An operator has stood this up and no
            // admin has made a surface public yet, and the message is the one
            // thing that tells them so rather than leaving them looking for a
            // bug.
            None => Err(Refusal::not_found(
                "public surface. This shell is open to anybody, so it shows only \
                 surfaces an admin made public; make one public from a shell \
                 configured with an authenticator, against this same database",
            )),
        };
    }

    let existing = state
        .store
        .layouts_owned_by(&principal.sub)
        .await
        .map_err(Refusal::store)?;

    if let Some(layout) = existing.into_iter().next() {
        return Ok(Json(document_for(&layout, &principal.sub)));
    }

    if let Some(layout) = org_default(&state, &principal.sub).await? {
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

/// The org default, if one is set, is still published or public, and this
/// principal may view it.
///
/// A default whose owner has since made it personal is not served: it is no
/// longer something everyone may see.
async fn org_default(state: &AppState, principal: &str) -> Result<Option<Layout>, Refusal> {
    let Some(id) = state.store.org_default().await.map_err(Refusal::store)? else {
        return Ok(None);
    };
    let layout = state.store.layout(id).await.map_err(Refusal::store)?;
    Ok(layout.filter(|layout| {
        layout.is_shared() && layout.is_visible_to(principal, state.config.read_only())
    }))
}

// -- The org default -------------------------------------------------------

/// The org default surface, for anyone who may view it.
pub async fn read_default(
    State(state): State<AppState>,
    Caller(principal): Caller,
) -> Answer<LayoutDocument> {
    match org_default(&state, &principal.sub).await? {
        Some(layout) => Ok(Json(document_for(&layout, &principal.sub))),
        None => Err(Refusal::not_found("org default")),
    }
}

/// What an admin sends to set the org default.
#[derive(Debug, serde::Deserialize)]
pub struct DefaultRequest {
    /// The layout to make the default.
    pub id: String,
}

/// Set the org default. Admin only; the layout must be published or public.
pub async fn set_default(
    State(state): State<AppState>,
    Author(principal): Author,
    IsAdmin(admin): IsAdmin,
    Json(request): Json<DefaultRequest>,
) -> Answer<LayoutDocument> {
    if !admin {
        return Err(Refusal::forbidden("only an admin may set the org default"));
    }

    let uuid = identifier(&request.id)?;
    let layout = state
        .store
        .layout(uuid)
        .await
        .map_err(Refusal::store)?
        .ok_or_else(|| Refusal::not_found("layout"))?;

    if !layout.is_shared() {
        return Err(Refusal::unusable(
            "the org default must be published or public; this layout is personal".to_string(),
        ));
    }

    state
        .store
        .set_org_default(Some(uuid))
        .await
        .map_err(Refusal::store)?;

    tracing::info!(admin = principal.sub, layout = %uuid, "set the org default");
    Ok(Json(document_for(&layout, &principal.sub)))
}

/// Clear the org default. Admin only.
pub async fn clear_default(
    State(state): State<AppState>,
    Author(principal): Author,
    IsAdmin(admin): IsAdmin,
) -> Result<StatusCode, Refusal> {
    if !admin {
        return Err(Refusal::forbidden(
            "only an admin may clear the org default",
        ));
    }

    state
        .store
        .set_org_default(None)
        .await
        .map_err(Refusal::store)?;

    tracing::info!(admin = principal.sub, "cleared the org default");
    Ok(StatusCode::NO_CONTENT)
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
    Author(principal): Author,
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
/// taken as the arrangement in full: items it omitted are gone, and items it
/// added arrive without identifiers, which are assigned here.
///
/// Read from plain JSON first, so an item of a type this shell does not know
/// is refused with a reason that names it.
pub async fn replace(
    State(state): State<AppState>,
    Author(caller): Author,
    IsAdmin(admin): IsAdmin,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Answer<LayoutDocument> {
    let principal = caller.sub.clone();
    let uuid = identifier(&id)?;
    let document = read_document(body)?;

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
        // Making a layout public shows it to anybody at all, so only an admin
        // may (HLIN-A-0016). Keeping one public that already is, or taking it
        // back to published or personal, is the owner's own business.
        if visibility == Visibility::Public && stored.visibility != Visibility::Public && !admin {
            return Err(Refusal::forbidden(
                "only an admin may make a layout public. Publish it to share it with \
                 everyone signed in",
            ));
        }
        stored.visibility = visibility;
    }

    stored.items = items_from(&document, &stored)?;

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
    Author(principal): Author,
    IsAdmin(admin): IsAdmin,
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

    // The owner, or an admin removing what someone else shared
    // (HLIN-A-0016). A personal layout is its owner's alone.
    let removable = stored.is_editable_by(&principal) || (admin && stored.is_shared());
    if !removable {
        return Err(Refusal::forbidden("this layout belongs to someone else"));
    }
    if !stored.is_editable_by(&principal) {
        tracing::info!(
            admin = principal,
            owner = stored.owner,
            layout = %uuid,
            "an admin deleted a shared layout"
        );
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
    Author(principal): Author,
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

    if !layout.is_visible_to(principal, state.config.read_only()) {
        // Here a 404 is right: a layout this principal may not see is not
        // theirs to know about. On an anonymous shell that is every layout
        // that is not public.
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
        panel_count: layout.panels().count(),
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
        items: layout.items.iter().map(item_document).collect(),
    }
}

fn item_document(item: &LayoutItem) -> ItemDocument {
    match item {
        LayoutItem::Panel(panel) => ItemDocument::Panel(instance_document(panel)),
        LayoutItem::Bar(bar) => ItemDocument::Bar(bar.clone()),
        LayoutItem::PageSelector(selector) => ItemDocument::PageSelector(selector.clone()),
        LayoutItem::Content(content) => ItemDocument::Content(content.clone()),
    }
}

/// A layout as the browser wrote it, or the reason it cannot be read.
fn read_document(body: serde_json::Value) -> Result<LayoutDocument, Refusal> {
    if body.get("panels").is_some() && body.get("items").is_none() {
        return Err(Refusal::unusable(
            "`panels` is now `items`: send a list of items, each with a `type`".to_string(),
        ));
    }

    if let Some(items) = body.get("items").and_then(|items| items.as_array()) {
        for (index, item) in items.iter().enumerate() {
            match item.get("type").and_then(|kind| kind.as_str()) {
                Some(kind) if ItemDocument::TYPES.contains(&kind) => {}
                Some(kind) => {
                    return Err(Refusal::unusable(format!(
                        "item {index}: `{kind}` is not an item type. Known types: {}",
                        ItemDocument::TYPES.join(", ")
                    )));
                }
                None => {
                    return Err(Refusal::unusable(format!(
                        "item {index} has no `type`. Known types: {}",
                        ItemDocument::TYPES.join(", ")
                    )));
                }
            }
        }
    }

    serde_json::from_value(body)
        .map_err(|error| Refusal::unusable(format!("the layout could not be read: {error}")))
}

/// The items to store, from what the browser sent.
///
/// A panel's identity is the shell's to assign (see `instance_from`). A shell
/// component keeps the id it was given, because a selector names its content
/// item by that id; one sent without an id gets one here. Two items may not
/// share an id.
fn items_from(document: &LayoutDocument, layout: &Layout) -> Result<Vec<LayoutItem>, Refusal> {
    let mut items = Vec::with_capacity(document.items.len());
    for item in &document.items {
        let stored = match item {
            ItemDocument::Panel(panel) => LayoutItem::Panel(instance_from(panel, layout)?),
            other => {
                let mut component = other.clone();
                let id = component
                    .id()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                component.set_id(Some(id));
                let position = component.position().clamped();
                *component.position_mut() = position;
                match component {
                    ItemDocument::Bar(bar) => LayoutItem::Bar(bar),
                    ItemDocument::PageSelector(selector) => LayoutItem::PageSelector(selector),
                    ItemDocument::Content(content) => LayoutItem::Content(content),
                    ItemDocument::Panel(_) => unreachable!("panels are matched above"),
                }
            }
        };
        items.push(stored);
    }

    let mut seen = std::collections::BTreeSet::new();
    for item in &items {
        let id = item_id(item);
        if !seen.insert(id.clone()) {
            return Err(Refusal::unusable(format!("two items have the id `{id}`")));
        }
    }

    Ok(items)
}

fn item_id(item: &LayoutItem) -> String {
    match item {
        LayoutItem::Panel(panel) => panel.id.to_string(),
        LayoutItem::Bar(bar) => bar.id.clone().unwrap_or_default(),
        LayoutItem::PageSelector(selector) => selector.id.clone().unwrap_or_default(),
        LayoutItem::Content(content) => content.id.clone().unwrap_or_default(),
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
        .and_then(|id| layout.panels().find(|panel| panel.id == id));

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
        let items = panels.into_iter().map(LayoutItem::from).collect();
        Layout {
            id: Uuid::new_v4(),
            owner: "ada".to_string(),
            title: "Mine".to_string(),
            visibility: Visibility::Personal,
            forked_from: None,
            time_range: None,
            items,
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

    fn refusal_of(body: serde_json::Value) -> String {
        read_document(body).expect_err("refused").reason
    }

    #[test]
    fn an_unknown_item_type_is_refused_with_a_reason() {
        let reason = refusal_of(serde_json::json!({
            "title": "Mine", "visibility": "personal",
            "items": [ { "type": "bar" }, { "type": "clock" } ]
        }));
        assert!(reason.contains("item 1"), "{reason}");
        assert!(reason.contains("`clock`"), "{reason}");
        assert!(
            reason.contains("page-selector"),
            "it names the known types: {reason}"
        );

        let reason = refusal_of(serde_json::json!({
            "title": "Mine", "visibility": "personal", "items": [ { "id": "x" } ]
        }));
        assert!(reason.contains("no `type`"), "{reason}");
    }

    #[test]
    fn the_old_panels_list_is_refused_rather_than_dropped() {
        let reason = refusal_of(serde_json::json!({
            "title": "Mine", "visibility": "personal", "panels": []
        }));
        assert!(reason.contains("`items`"), "{reason}");
    }

    #[test]
    fn a_shell_component_keeps_its_id_and_one_without_gets_one() {
        let layout = layout_with(vec![]);
        let document = read_document(serde_json::json!({
            "title": "Mine", "visibility": "personal",
            "items": [
                { "type": "content", "id": "main", "position": { "x": 2, "y": 0, "w": 10, "h": 6 } },
                { "type": "page-selector", "target": "main",
                  "position": { "x": 0, "y": 0, "w": 30, "h": 6 } }
            ]
        }))
        .expect("a layout");

        let items = items_from(&document, &layout).expect("items");
        assert_eq!(item_id(&items[0]), "main");
        let LayoutItem::PageSelector(selector) = &items[1] else {
            panic!("a selector");
        };
        assert!(selector.id.as_deref().is_some_and(|id| !id.is_empty()));
        assert_eq!(
            selector.position.w, 12,
            "a position is forced into the grid"
        );
    }

    #[test]
    fn two_items_may_not_share_an_id() {
        let layout = layout_with(vec![]);
        let document = read_document(serde_json::json!({
            "title": "Mine", "visibility": "personal",
            "items": [ { "type": "bar", "id": "top" }, { "type": "content", "id": "top" } ]
        }))
        .expect("a layout");
        let reason = items_from(&document, &layout).expect_err("refused").reason;
        assert!(reason.contains("`top`"), "{reason}");
    }
}
