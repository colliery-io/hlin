//! An in-process store, for tests.
//!
//! This is a test double, not a second supported backend (decision
//! HLIN-A-0006). It exists so the shell's logic can be tested without a
//! database, and it is held to the same behaviour as the real one by the shared
//! suite in `tests/store_suite`. Anything it does that Postgres would not is a
//! bug in this file.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::types::{
    Layout, NewLayout, PendingLogin, PlatformSnapshot, Session, Violation, Visibility,
};
use super::{Result, Store, StoreError};

/// A store held in memory.
#[derive(Debug, Default)]
pub struct MemoryStore {
    platforms: Mutex<HashMap<String, PlatformSnapshot>>,
    layouts: Mutex<HashMap<Uuid, Layout>>,
    sessions: Mutex<HashMap<String, Session>>,
    pending: Mutex<HashMap<String, PendingLogin>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn platforms(&self) -> std::sync::MutexGuard<'_, HashMap<String, PlatformSnapshot>> {
        self.platforms.lock().expect("store lock is not poisoned")
    }

    fn layouts(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Layout>> {
        self.layouts.lock().expect("store lock is not poisoned")
    }
}

fn most_recent_first(mut layouts: Vec<Layout>) -> Vec<Layout> {
    layouts.sort_by_key(|layout| std::cmp::Reverse(layout.updated_at));
    layouts
}

#[async_trait]
impl Store for MemoryStore {
    async fn platform_snapshot(&self, platform_id: &str) -> Result<Option<PlatformSnapshot>> {
        Ok(self.platforms().get(platform_id).cloned())
    }

    async fn platform_snapshots(&self) -> Result<Vec<PlatformSnapshot>> {
        let mut all: Vec<PlatformSnapshot> = self.platforms().values().cloned().collect();
        all.sort_by(|left, right| left.platform_id.cmp(&right.platform_id));
        Ok(all)
    }

    async fn observe_platform(&self, snapshot: PlatformSnapshot) -> Result<PlatformSnapshot> {
        let mut platforms = self.platforms();
        let carried = platforms
            .get(&snapshot.platform_id)
            .filter(|previous| previous.contract_hash == snapshot.contract_hash)
            .map(|previous| previous.consecutive_observations)
            .unwrap_or(0);

        let stored = PlatformSnapshot {
            consecutive_observations: carried + 1,
            ..snapshot
        };
        platforms.insert(stored.platform_id.clone(), stored.clone());
        Ok(stored)
    }

    async fn record_violation(
        &self,
        platform_id: &str,
        declared: &str,
        expected_major: u64,
        changes: Vec<String>,
    ) -> Result<Violation> {
        let mut platforms = self.platforms();
        let seen = platforms
            .get(platform_id)
            .and_then(|snapshot| snapshot.last_violation.as_ref())
            .map(|previous| previous.seen)
            .unwrap_or(0);

        let violation = Violation {
            declared: declared.to_string(),
            expected_major,
            changes,
            at: Utc::now(),
            seen: seen + 1,
        };

        if let Some(snapshot) = platforms.get_mut(platform_id) {
            snapshot.last_violation = Some(violation.clone());
        }

        Ok(violation)
    }

    async fn forget_platform(&self, platform_id: &str) -> Result<bool> {
        Ok(self.platforms().remove(platform_id).is_some())
    }

    async fn layout(&self, id: Uuid) -> Result<Option<Layout>> {
        Ok(self.layouts().get(&id).cloned())
    }

    async fn layouts_owned_by(&self, owner: &str) -> Result<Vec<Layout>> {
        let owned = self
            .layouts()
            .values()
            .filter(|layout| layout.owner == owner)
            .cloned()
            .collect();
        Ok(most_recent_first(owned))
    }

    async fn published_layouts(&self) -> Result<Vec<Layout>> {
        let published = self
            .layouts()
            .values()
            .filter(|layout| layout.is_published())
            .cloned()
            .collect();
        Ok(most_recent_first(published))
    }

    async fn create_layout(&self, new: NewLayout) -> Result<Layout> {
        let now = Utc::now();
        let layout = Layout {
            id: Uuid::new_v4(),
            owner: new.owner,
            title: new.title,
            visibility: new.visibility,
            forked_from: new.forked_from,
            time_range: new.time_range,
            panels: new.panels,
            created_at: now,
            updated_at: now,
        };
        self.layouts().insert(layout.id, layout.clone());
        Ok(layout)
    }

    async fn update_layout(&self, layout: &Layout) -> Result<()> {
        let mut layouts = self.layouts();
        if !layouts.contains_key(&layout.id) {
            return Err(StoreError::NotFound {
                entity: "layout",
                id: layout.id.to_string(),
            });
        }
        let mut stored = layout.clone();
        stored.updated_at = Utc::now();
        layouts.insert(stored.id, stored);
        Ok(())
    }

    async fn delete_layout(&self, id: Uuid) -> Result<bool> {
        let mut layouts = self.layouts();
        let existed = layouts.remove(&id).is_some();

        // A fork outlives the layout it came from; only its provenance is lost.
        for layout in layouts.values_mut() {
            if layout.forked_from == Some(id) {
                layout.forked_from = None;
            }
        }
        Ok(existed)
    }

    async fn fork_layout(&self, id: Uuid, new_owner: &str) -> Result<Layout> {
        let original = self.layout(id).await?.ok_or_else(|| StoreError::NotFound {
            entity: "layout",
            id: id.to_string(),
        })?;

        // A fork is its owner's own, and starts personal however the original
        // was shared: publishing is a decision its new owner makes.
        self.create_layout(NewLayout {
            owner: new_owner.to_string(),
            title: original.title.clone(),
            visibility: Visibility::Personal,
            forked_from: Some(original.id),
            time_range: original.time_range.clone(),
            panels: original
                .panels
                .iter()
                .map(|panel| super::types::PanelInstance {
                    id: Uuid::new_v4(),
                    ..panel.clone()
                })
                .collect(),
        })
        .await
    }

    // -- Sessions ---------------------------------------------------------

    async fn begin_login(&self, pending: PendingLogin) -> Result<()> {
        self.pending
            .lock()
            .expect("store lock is not poisoned")
            .insert(pending.state.clone(), pending);
        Ok(())
    }

    async fn claim_login(&self, state: &str) -> Result<Option<PendingLogin>> {
        let claimed = self
            .pending
            .lock()
            .expect("store lock is not poisoned")
            .remove(state);

        // Removed either way: an expired sign-in is finished with, and leaving
        // the row would only mean sweeping it later.
        Ok(claimed.filter(|pending| Utc::now() < pending.expires_at))
    }

    async fn create_session(&self, session: Session) -> Result<()> {
        self.sessions
            .lock()
            .expect("store lock is not poisoned")
            .insert(session.id.clone(), session);
        Ok(())
    }

    async fn session(&self, id: &str) -> Result<Option<Session>> {
        let found = self
            .sessions
            .lock()
            .expect("store lock is not poisoned")
            .get(id)
            .cloned();
        Ok(found.filter(|session| session.is_current_at(Utc::now())))
    }

    async fn end_session(&self, id: &str) -> Result<bool> {
        Ok(self
            .sessions
            .lock()
            .expect("store lock is not poisoned")
            .remove(id)
            .is_some())
    }

    async fn sweep_expired(&self) -> Result<u64> {
        let now = Utc::now();
        let mut swept = 0u64;

        let mut sessions = self.sessions.lock().expect("store lock is not poisoned");
        let before = sessions.len();
        sessions.retain(|_, session| session.is_current_at(now));
        swept += (before - sessions.len()) as u64;
        drop(sessions);

        let mut pending = self.pending.lock().expect("store lock is not poisoned");
        let before = pending.len();
        pending.retain(|_, login| now < login.expires_at);
        swept += (before - pending.len()) as u64;

        Ok(swept)
    }
}
