//! The surfaces currently being served.
//!
//! One per `(layout, principal)`, because a surface is a viewer's view of a
//! layout: two people looking at the same layout are two surfaces, since a
//! platform may legitimately answer them differently (decision HLIN-A-0004).

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::identity::{Carried, Viewer};
use crate::server::AppState;
use crate::stream::aggregator::{Instance, Surface};
use crate::stream::live::LiveSurface;

/// A layout, resolved into something the aggregator can run.
///
/// The two travel together because they come from the same document and are
/// meaningless apart: a selection names an instance, and an instance without
/// its viewer's choices is not the panel they composed.
struct Composition {
    instances: Vec<Instance>,
    selections: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// Every surface this shell is currently serving.
#[derive(Default)]
pub struct Surfaces {
    live: Mutex<BTreeMap<String, Arc<LiveSurface>>>,
}

impl Surfaces {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The surface for this layout and viewer, starting it if it is not
    /// already running.
    pub async fn for_surface(
        self: &Arc<Self>,
        surface_id: &str,
        principal: &hlin_identity::Principal,
        state: &AppState,
        headers: &axum::http::HeaderMap,
    ) -> Result<Arc<LiveSurface>, String> {
        let key = format!("{surface_id}:{}", principal.sub);

        // The lock is held across the whole of this, including the store and
        // registry reads, and that is the point rather than an oversight.
        //
        // Checking the map, releasing the lock, building a surface and then
        // inserting it lets two requests that arrive together both miss and
        // both build. The second insert wins the map and the first surface goes
        // on running with nobody able to find it — while a browser that
        // subscribed to it sits attached to a surface the shell has forgotten,
        // receiving frames from a generation it has already moved past and
        // therefore discarding every one.
        //
        // That is not hypothetical: the browser sends its parameters and opens
        // its stream from the same tick, so those two requests arrive together
        // every time a layout is written. It cost an afternoon to find, and it
        // costs one lock to prevent.
        let mut live = self.live.lock().await;

        if let Some(existing) = live.get(&key) {
            return Ok(existing.clone());
        }

        let composition = self.instances_for(surface_id, principal, state).await?;

        let viewer = Viewer {
            principal: principal.clone(),
            carried: Carried::from_cookie_header(
                headers
                    .get(axum::http::header::COOKIE)
                    .and_then(|value| value.to_str().ok()),
            ),
        };

        let mut surface = Surface::new(composition.instances, state.config.timings.policy());
        surface.restore_selections(composition.selections);
        let running = LiveSurface::new(
            surface,
            state.registry.clone(),
            viewer,
            // The shell's own, not one per surface. Building a client here gave
            // every surface its own connection pool to platforms they mostly
            // share — and, once there was a trust setting, would have been a
            // fourth place to remember to apply it.
            state.client.clone(),
            state.stream_client.clone(),
            state.streams.clone(),
            state.config.timings.tick(),
        );

        // The driver is given a way home so it can take itself out of this map
        // when nobody is left watching. `Weak`, because this map holds the
        // surface: an `Arc` both ways is a cycle that frees neither.
        tokio::spawn(running.clone().run(Arc::downgrade(self), key.clone()));

        live.insert(key, running.clone());
        Ok(running)
    }

    /// Stop serving one surface, if it is still unwatched when the lock is
    /// taken.
    ///
    /// Called by a driver that has gone the idle grace without a subscriber.
    /// The check is made again here, under the same lock `for_surface` takes,
    /// because a browser can subscribe between the driver deciding to stop and
    /// this acquiring the lock — and removing a surface somebody has just
    /// attached to would strand them exactly as the orphaned-surface bug did.
    ///
    /// Returns whether the driver should stop.
    pub async fn release(&self, key: &str, running: &Arc<LiveSurface>) -> bool {
        let mut live = self.live.lock().await;

        let Some(held) = live.get(key) else {
            // Already gone — a layout write dropped it. Nothing to remove, and
            // the driver has no reason to keep running.
            return true;
        };

        if !Arc::ptr_eq(held, running) {
            // The map holds a different surface under this key now. This
            // driver is the old one and should stop without touching it.
            return true;
        }

        if running.viewers() > 0 {
            return false;
        }

        live.remove(key);
        true
    }

    /// Bring every surface built from this layout into line with what was just
    /// written.
    ///
    /// Replaces `forget` on the write path. Dropping the surface was correct
    /// and expensive: it meant the next subscription rebuilt from the store and
    /// every panel refetched, so moving one panel a single cell blanked the
    /// whole surface until the answers came back.
    ///
    /// Returns how many surfaces were reconciled — one per viewer watching this
    /// layout.
    pub async fn reconcile(
        &self,
        surface_id: &str,
        principal: &hlin_identity::Principal,
        state: &AppState,
    ) -> usize {
        let prefix = format!("{surface_id}:");

        // The composition is read once, outside the map's lock: it touches the
        // store and the registry, and holding the lock across those is what
        // `for_surface` does deliberately for a *different* reason — there, two
        // callers must not both build. Here there is nothing to race.
        let composition = match self.instances_for(surface_id, principal, state).await {
            Ok(composition) => composition,
            Err(reason) => {
                // The layout is gone or no longer visible. Dropping is right.
                tracing::debug!(surface = surface_id, %reason, "cannot reconcile; dropping instead");
                return self.forget(surface_id).await;
            }
        };

        let watching: Vec<Arc<LiveSurface>> = {
            let live = self.live.lock().await;
            live.iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, surface)| surface.clone())
                .collect()
        };

        for surface in &watching {
            surface
                .replace_instances(
                    composition.instances.clone(),
                    composition.selections.clone(),
                )
                .await;
        }

        watching.len()
    }

    /// Stop serving every surface built from this layout.
    ///
    /// Called when the layout is written or deleted. A running surface holds
    /// the panels the layout had when it started, so the only honest thing to
    /// do with one whose layout has changed is drop it and let the next
    /// subscription build from what was written. Returns how many were
    /// dropped, which is one per viewer currently watching it.
    pub async fn forget(&self, surface_id: &str) -> usize {
        let prefix = format!("{surface_id}:");
        let mut live = self.live.lock().await;
        let doomed: Vec<String> = live
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in &doomed {
            live.remove(key);
        }
        doomed.len()
    }

    /// The panels on a surface.
    ///
    /// A surface id is a layout id, with one exception: `all` is every panel
    /// every platform currently offers, which is what the walkthrough and a
    /// `curl` of the stream use to see the aggregator without composing
    /// anything first.
    async fn instances_for(
        &self,
        surface_id: &str,
        principal: &hlin_identity::Principal,
        state: &AppState,
    ) -> Result<Composition, String> {
        if surface_id == "all" {
            return Ok(Composition {
                instances: Self::everything(state).await,
                selections: BTreeMap::new(),
            });
        }

        let id = uuid::Uuid::parse_str(surface_id)
            .map_err(|_| format!("no surface named `{surface_id}`"))?;

        let layout = state
            .store
            .layout(id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("no layout `{surface_id}`"))?;

        if !layout.is_visible_to(&principal.sub) {
            return Err(format!("layout `{surface_id}` is not visible"));
        }

        let views = state.registry.views().await;
        let mut instances = Vec::new();
        let mut selections: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

        for panel in &layout.panels {
            // What the viewer chose, stored with the panel. A selection that no
            // longer parses is dropped rather than refused: the panel is still
            // drawable without it, and refusing the whole surface over one
            // unreadable choice would be a poor trade.
            match serde_json::from_value::<BTreeMap<String, Vec<String>>>(panel.selections.clone())
            {
                Ok(chosen) if !chosen.is_empty() => {
                    selections.insert(panel.id.to_string(), chosen);
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(
                    panel = %panel.id,
                    %error,
                    "a stored selection could not be read and was ignored"
                ),
            }

            let declared = views.get(&panel.platform_id).and_then(|view| {
                view.accepted_panels()
                    .into_iter()
                    .find(|declared| declared.key == panel.panel_key)
                    .cloned()
            });

            // A layout outlives the panels on it. When a platform stops
            // offering one, the panel does not disappear from the surface: it
            // says it is gone, which is the difference between a shell that
            // degrades and one that silently loses a person's work.
            let Some(declared) = declared else {
                instances.push(Instance::retired(
                    panel.id.to_string(),
                    panel.platform_id.clone(),
                    panel.panel_key.clone(),
                ));
                continue;
            };

            let mut instance = Instance::new(
                panel.id.to_string(),
                panel.platform_id.clone(),
                panel.panel_key.clone(),
                declared.data.clone(),
                declared.envelope.clone(),
            );
            instance.successor.clone_from(&declared.lifecycle.successor);

            // What this panel said it responds to. A declaration with no `id`
            // names no query key — `time_range` is driven by the surface's one
            // picker — so it grants nothing.
            instance.refresh = declared
                .refresh_ms
                .map(|ms| chrono::Duration::milliseconds(ms.min(i64::MAX as u64) as i64));
            instance.pushed = declared.pushed;
            instance.accepts = declared
                .params
                .iter()
                .filter_map(|declaration| declaration.config.get("id"))
                .filter_map(|id| id.as_str())
                .map(str::to_string)
                .collect();

            instances.push(instance);
        }

        Ok(Composition {
            instances,
            selections,
        })
    }

    /// Every panel every platform offers, as one surface.
    async fn everything(state: &AppState) -> Vec<Instance> {
        let views = state.registry.views().await;
        let mut instances = Vec::new();

        for view in views.values() {
            for panel in view.accepted_panels() {
                let mut instance = Instance::new(
                    format!("{}-{}", view.config.id, panel.key),
                    view.config.id.clone(),
                    panel.key.clone(),
                    panel.data.clone(),
                    panel.envelope.clone(),
                );
                instance.successor.clone_from(&panel.lifecycle.successor);
                instance.refresh = panel
                    .refresh_ms
                    .map(|ms| chrono::Duration::milliseconds(ms.min(i64::MAX as u64) as i64));
                instance.pushed = panel.pushed;
                instance.accepts = panel
                    .params
                    .iter()
                    .filter_map(|declaration| declaration.config.get("id"))
                    .filter_map(|id| id.as_str())
                    .map(str::to_string)
                    .collect();
                instances.push(instance);
            }
        }

        instances
    }
}
