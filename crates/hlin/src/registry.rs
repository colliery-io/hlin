//! What the shell knows about platforms, and how it notices a change.
//!
//! The registry holds the sequence of manifests for each platform, which is
//! what makes it the component that can diff them. That is where the shell
//! acts as its own continuous integration: no external service gates a
//! platform's release, and a breaking change shipped without a major bump is
//! caught on the running system within a poll (decision HLIN-A-0002).
//!
//! Two rules keep the mechanism from crying wolf, both implemented here. A
//! change must be observed on several consecutive polls before it is
//! classified, so a blue/green rollout flapping between revisions does not
//! raise a violation per flip. And a version that goes backwards is a rollback,
//! logged and never flagged, because rollbacks happen during incidents.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use hlin_manifest::{DiffReport, Manifest, Validation, Verdict, classify_diff, contract_hash};
use tokio::sync::RwLock;

use crate::config::{Config, PlatformConfig};
use crate::identity::Credentialer;
use crate::manifest_client::{Fetched, ManifestClient};
use crate::store::{PlatformSnapshot, Store};

/// What the shell currently believes about one platform.
pub struct PlatformView {
    /// As configured.
    pub config: PlatformConfig,

    /// The credential the shell attaches when calling it.
    pub credentialer: Box<dyn Credentialer>,

    /// The last manifest that could be read, whether or not it was valid.
    ///
    /// This is what panels are offered from, so it moves on every successful
    /// poll: a viewer should see what a platform is serving now.
    pub manifest: Option<Manifest>,

    /// The last manifest whose contract was actually judged.
    ///
    /// Deliberately separate from `manifest`, and deliberately lagging. A
    /// change is only believed once it has been observed on enough consecutive
    /// polls, and until then this must not move: if the baseline advanced with
    /// every poll, the second sighting of a change would compare against the
    /// first and find nothing, so the change would be applied and never judged.
    classified: Option<Manifest>,

    /// What validation made of it.
    pub validation: Option<Validation>,

    /// Whether the platform answered the last time it was asked.
    pub reachable: bool,

    /// Why not, when it did not.
    pub trouble: Option<String>,

    /// The fingerprint of the last contract that was classified.
    pub contract_hash: Option<String>,

    /// The last breaking change this platform shipped without declaring it.
    ///
    /// Restored on boot alongside the contract memory, so a violation survives
    /// a deploy — which matters, because the window a violation is most likely
    /// to be shipped in is the one nobody is watching.
    pub last_violation: Option<crate::store::Violation>,

    /// How many consecutive polls have agreed on the current contract.
    pub consecutive_observations: i32,
}

impl PlatformView {
    /// The panels a viewer may put on a surface.
    pub fn accepted_panels(&self) -> Vec<&hlin_manifest::Panel> {
        let (Some(manifest), Some(validation)) = (&self.manifest, &self.validation) else {
            return Vec::new();
        };
        if !validation.is_document_valid() {
            return Vec::new();
        }

        validation
            .panels
            .iter()
            .filter(|outcome| outcome.is_accepted())
            .filter_map(|outcome| manifest.panel(&outcome.key))
            .collect()
    }

    /// A panel by key, if this platform currently offers it.
    pub fn panel(&self, key: &str) -> Option<&hlin_manifest::Panel> {
        self.accepted_panels()
            .into_iter()
            .find(|panel| panel.key == key)
    }

    /// The panel served by an endpoint.
    ///
    /// Two panels may share one endpoint, which is what makes deduplication
    /// worth doing; they must agree on the envelope, since that is what the
    /// shell insists the response matches. The first is therefore as good as
    /// any.
    pub fn panel_by_endpoint(&self, endpoint: &str) -> Option<&hlin_manifest::Panel> {
        self.accepted_panels()
            .into_iter()
            .find(|panel| panel.data == endpoint)
    }
}

/// Everything the shell knows about every platform.
pub struct Registry {
    views: RwLock<BTreeMap<String, PlatformView>>,
    store: Arc<dyn Store>,
    client: Arc<dyn ManifestClient>,
    debounce: i32,
}

impl Registry {
    /// A registry for these platforms, with nothing observed yet.
    pub fn new(
        config: &Config,
        issuer: Arc<hlin_identity::Issuer>,
        store: Arc<dyn Store>,
        client: Arc<dyn ManifestClient>,
    ) -> Result<Self, String> {
        let mut views = BTreeMap::new();

        for platform in &config.platforms {
            let credentialer = crate::identity::build(&platform.auth, &platform.id, issuer.clone())
                .map_err(|reason| format!("platform `{}`: {reason}", platform.id))?;

            if credentialer.collapses_principals() {
                tracing::warn!(
                    platform = platform.id,
                    "using {}: every viewer reaches this platform as the same caller, so its \
                     own per-user rules cannot apply, no viewer can be told they lack access, \
                     and requests are deduplicated across everyone",
                    credentialer.name()
                );
            }

            views.insert(
                platform.id.clone(),
                PlatformView {
                    config: platform.clone(),
                    credentialer,
                    manifest: None,
                    classified: None,
                    validation: None,
                    reachable: false,
                    trouble: None,
                    contract_hash: None,
                    last_violation: None,
                    consecutive_observations: 0,
                },
            );
        }

        Ok(Self {
            views: RwLock::new(views),
            store,
            client,
            debounce: config.timings.debounce_observations.max(1),
        })
    }

    /// Load what was known before a restart.
    ///
    /// Without this the shell forgets every contract on every deploy, and a
    /// breaking change shipped across the restart window is never noticed.
    pub async fn restore(&self) -> Result<usize, String> {
        let snapshots = self
            .store
            .platform_snapshots()
            .await
            .map_err(|error| error.to_string())?;

        let mut views = self.views.write().await;
        let mut restored = 0;

        for snapshot in snapshots {
            let Some(view) = views.get_mut(&snapshot.platform_id) else {
                // Configuration no longer lists this platform. Its snapshot is
                // left in the store rather than deleted, so re-adding it later
                // does not look like a brand new platform.
                continue;
            };

            view.contract_hash = Some(snapshot.contract_hash.clone());
            view.last_violation = snapshot.last_violation.clone();
            view.consecutive_observations = snapshot.consecutive_observations;
            view.manifest = serde_json::from_value(snapshot.manifest).ok();
            // What was remembered is by definition what was last judged, so a
            // change shipped across the restart is compared against it.
            view.classified = view.manifest.clone();
            if let Some(manifest) = &view.manifest {
                view.validation = Some(hlin_manifest::validate(manifest, &snapshot.platform_id));
            }
            restored += 1;
        }

        Ok(restored)
    }

    /// Ask every platform once.
    pub async fn poll_all(&self) {
        let bases: Vec<(String, String)> = {
            let views = self.views.read().await;
            views
                .values()
                .map(|view| (view.config.id.clone(), view.config.base_url.clone()))
                .collect()
        };

        for (id, base_url) in bases {
            self.poll_one(&id, &base_url).await;
        }
    }

    /// Ask one platform, and act on what comes back.
    pub async fn poll_one(&self, platform_id: &str, base_url: &str) {
        let fetched = self.client.fetch(base_url).await;

        match fetched {
            Fetched::Unreachable { reason } => {
                tracing::debug!(platform = platform_id, %reason, "platform did not answer");
                let mut views = self.views.write().await;
                if let Some(view) = views.get_mut(platform_id) {
                    view.reachable = false;
                    view.trouble = Some(reason);
                    // The last known contract stays in place, so panels on an
                    // existing surface degrade rather than disappear.
                }
            }

            Fetched::Unreadable { reason } => {
                tracing::warn!(platform = platform_id, %reason, "platform served an unreadable manifest");
                let mut views = self.views.write().await;
                if let Some(view) = views.get_mut(platform_id) {
                    view.reachable = true;
                    view.trouble = Some(reason);
                }
            }

            Fetched::Document(manifest) => self.observe(platform_id, *manifest).await,
        }
    }

    async fn observe(&self, platform_id: &str, manifest: Manifest) {
        let validation = hlin_manifest::validate(&manifest, platform_id);

        if let Some(defect) = &validation.document {
            tracing::warn!(platform = platform_id, %defect, "manifest is malformed");
            let mut views = self.views.write().await;
            if let Some(view) = views.get_mut(platform_id) {
                view.reachable = true;
                view.trouble = Some(defect.to_string());
                view.validation = Some(validation);
            }
            return;
        }

        for (key, defect) in validation.rejected() {
            tracing::warn!(platform = platform_id, panel = key, %defect, "panel rejected");
        }

        if let Some(newer) = validation.newer_schema_version {
            tracing::info!(
                platform = platform_id,
                declared = newer,
                supported = hlin_manifest::SUPPORTED_SCHEMA_VERSION,
                "platform declares a newer manifest format; reading it at the version this shell knows"
            );
        }

        let hash = contract_hash(&manifest);
        let previous = self.baseline(platform_id).await;

        // The store owns the observation count, so two shell instances polling
        // one platform cannot lose an observation between them.
        let snapshot = PlatformSnapshot {
            platform_id: platform_id.to_string(),
            manifest: serde_json::to_value(&manifest).unwrap_or(serde_json::Value::Null),
            contract_hash: hash.as_str().to_string(),
            contract_version: manifest.contract_version.to_string(),
            observed_at: Utc::now(),
            consecutive_observations: 1,
            // Carried forward by the store, not set here: whether this platform
            // has ever violated is a fact about its history, and an observation
            // must not erase it.
            last_violation: None,
        };

        let stored = match self.store.observe_platform(snapshot).await {
            Ok(stored) => stored,
            Err(error) => {
                tracing::error!(platform = platform_id, %error, "could not record what this platform is serving");
                return;
            }
        };

        let classified = self
            .classify(platform_id, previous.as_ref(), &manifest, &stored)
            .await;

        let mut views = self.views.write().await;
        if let Some(view) = views.get_mut(platform_id) {
            view.reachable = true;
            view.trouble = None;
            view.validation = Some(validation);
            view.consecutive_observations = stored.consecutive_observations;
            if stored.last_violation.is_some() {
                view.last_violation = stored.last_violation.clone();
            }
            if classified {
                view.contract_hash = Some(stored.contract_hash.clone());
                view.classified = Some(manifest.clone());
            }
            view.manifest = Some(manifest);
        }
    }

    /// The manifest a change is judged against: the last one that was judged,
    /// not the last one that was seen.
    async fn baseline(&self, platform_id: &str) -> Option<Manifest> {
        let views = self.views.read().await;
        views
            .get(platform_id)
            .and_then(|view| view.classified.clone())
    }

    /// Decide whether this contract has been seen enough to judge, and judge it.
    ///
    /// Returns whether the contract was classified, so the caller can record
    /// that this hash is now the one being compared against.
    async fn classify(
        &self,
        platform_id: &str,
        previous: Option<&Manifest>,
        next: &Manifest,
        stored: &PlatformSnapshot,
    ) -> bool {
        let Some(previous) = previous else {
            tracing::info!(
                platform = platform_id,
                version = %next.contract_version,
                "first contract seen for this platform"
            );
            return true;
        };

        let report = classify_diff(previous, next);
        if report.is_unchanged() {
            return true;
        }

        // A change is believed only once it has held. Below the threshold the
        // shell says nothing, which is what stops a flapping deployment from
        // raising a violation on every flip.
        if stored.consecutive_observations < self.debounce {
            tracing::debug!(
                platform = platform_id,
                seen = stored.consecutive_observations,
                needed = self.debounce,
                "a contract change is waiting to be believed"
            );
            return false;
        }

        self.report(platform_id, &report).await;
        true
    }

    async fn report(&self, platform_id: &str, report: &DiffReport) {
        match &report.verdict {
            Verdict::Violation {
                declared,
                expected_major,
            } => {
                // The shell renders the new manifest as delivered; the viewer
                // sees nothing about this. It is the platform team's problem
                // and an operator's to relay (decision HLIN-A-0002).
                tracing::error!(
                    platform = platform_id,
                    declared = %declared,
                    expected = format!("{expected_major}.0.0"),
                    changes = ?report.breaking(),
                    "contract violation: this platform shipped a breaking change without a major version bump"
                );

                // And written down, because a log line is a signal only for
                // somebody who happens to be watching. An operator arriving
                // afterwards needs to be able to ask whether this ever
                // happened, which is the whole point of the mechanism.
                let changes = report
                    .breaking()
                    .iter()
                    .map(|change| format!("{change:?}"))
                    .collect();

                match self
                    .store
                    .record_violation(platform_id, &declared.to_string(), *expected_major, changes)
                    .await
                {
                    Ok(violation) => {
                        if violation.seen > 1 {
                            tracing::error!(
                                platform = platform_id,
                                seen = violation.seen,
                                "this platform has now done this more than once"
                            );
                        }
                        // Onto the live view as well as into the store, so
                        // `/api/platforms` says so without waiting for a
                        // restart to read it back.
                        if let Some(view) = self.views.write().await.get_mut(platform_id) {
                            view.last_violation = Some(violation);
                        }
                    }
                    Err(error) => tracing::error!(
                        platform = platform_id,
                        %error,
                        "could not record the violation, so it exists only in this log line"
                    ),
                }
            }

            Verdict::Rollback { from, to } => {
                tracing::info!(
                    platform = platform_id,
                    %from,
                    %to,
                    "contract rolled back; applied without complaint"
                );
            }

            Verdict::Accepted => {
                tracing::debug!(
                    platform = platform_id,
                    class = ?report.class,
                    changes = report.changes.len(),
                    "contract changed"
                );
            }

            Verdict::NoChange => {}
        }
    }

    /// Read every platform's current view.
    pub async fn views(&self) -> tokio::sync::RwLockReadGuard<'_, BTreeMap<String, PlatformView>> {
        self.views.read().await
    }
}
