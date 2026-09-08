//! One suite, two implementations.
//!
//! The in-memory store is a test double rather than a supported backend
//! (decision HLIN-A-0006), and a test double is only useful while it behaves
//! like the real thing. Every case here runs against both, so a divergence
//! shows up as a failing test rather than as a bug that only appears in
//! production.
//!
//! The cases are written against the `Store` trait alone and know nothing about
//! either implementation.

use chrono::Utc;
use hlin::store::{
    Layout, NewLayout, PanelInstance, PendingLogin, PlatformSnapshot, Session, Store, Visibility,
};
use serde_json::json;
use uuid::Uuid;

/// Run every case against one store.
pub async fn run_all(store: &dyn Store) {
    contract_memory_starts_empty(store).await;
    an_unchanged_contract_accumulates_observations(store).await;
    a_changed_contract_starts_counting_again(store).await;
    a_forgotten_platform_is_gone(store).await;

    a_layout_round_trips_with_its_panels(store).await;
    a_missing_layout_is_not_an_error(store).await;
    layouts_are_listed_most_recently_changed_first(store).await;
    only_published_layouts_are_in_the_gallery(store).await;
    updating_a_layout_replaces_its_panels(store).await;
    updating_a_layout_that_is_gone_is_an_error(store).await;
    forking_gives_the_forker_their_own_copy(store).await;
    forking_something_that_is_gone_is_an_error(store).await;
    deleting_a_layout_takes_its_panels_but_spares_its_forks(store).await;

    a_session_round_trips(store).await;
    an_expired_session_is_nobody(store).await;
    ending_a_session_ends_it(store).await;
    a_sign_in_can_only_be_claimed_once(store).await;
    an_abandoned_sign_in_cannot_be_finished(store).await;
    sweeping_takes_only_what_is_over(store).await;
}

fn snapshot(platform: &str, hash: &str, version: &str) -> PlatformSnapshot {
    PlatformSnapshot {
        platform_id: platform.to_string(),
        manifest: json!({ "schema_version": 1, "platform": { "id": platform } }),
        contract_hash: hash.to_string(),
        contract_version: version.to_string(),
        observed_at: Utc::now(),
        consecutive_observations: 1,
        last_violation: None,
    }
}

fn panel(key: &str) -> PanelInstance {
    PanelInstance::new("orebank", key, json!({ "x": 0, "y": 0, "w": 6, "h": 4 }))
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

// -- Contract memory ------------------------------------------------------

async fn contract_memory_starts_empty(store: &dyn Store) {
    let unseen = unique("never-seen");
    assert!(
        store.platform_snapshot(&unseen).await.unwrap().is_none(),
        "a platform the shell has not met is None, not an error"
    );
}

async fn an_unchanged_contract_accumulates_observations(store: &dyn Store) {
    let platform = unique("steady");

    let first = store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();
    assert_eq!(first.consecutive_observations, 1);

    let second = store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();
    assert_eq!(
        second.consecutive_observations, 2,
        "an unchanged contract is the same observation again"
    );

    let third = store
        .observe_platform(snapshot(&platform, "hash-a", "1.1.0"))
        .await
        .unwrap();
    assert_eq!(
        third.consecutive_observations, 3,
        "the declared version moving is not the contract moving"
    );

    let read_back = store.platform_snapshot(&platform).await.unwrap().unwrap();
    assert_eq!(read_back.consecutive_observations, 3);
    assert_eq!(read_back.contract_version, "1.1.0");

    store.forget_platform(&platform).await.unwrap();
}

async fn a_changed_contract_starts_counting_again(store: &dyn Store) {
    let platform = unique("changing");

    store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();
    store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();

    let changed = store
        .observe_platform(snapshot(&platform, "hash-b", "2.0.0"))
        .await
        .unwrap();
    assert_eq!(
        changed.consecutive_observations, 1,
        "a new contract has been seen once, however long the old one stood"
    );

    // And flapping back is likewise one observation, which is what stops a
    // blue/green rollout from ever crossing a debounce threshold.
    let flapped = store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();
    assert_eq!(flapped.consecutive_observations, 1);

    store.forget_platform(&platform).await.unwrap();
}

async fn a_forgotten_platform_is_gone(store: &dyn Store) {
    let platform = unique("departing");
    store
        .observe_platform(snapshot(&platform, "hash-a", "1.0.0"))
        .await
        .unwrap();

    assert!(store.forget_platform(&platform).await.unwrap());
    assert!(store.platform_snapshot(&platform).await.unwrap().is_none());
    assert!(
        !store.forget_platform(&platform).await.unwrap(),
        "forgetting twice reports that there was nothing to forget"
    );
}

// -- Layouts --------------------------------------------------------------

async fn a_layout_round_trips_with_its_panels(store: &dyn Store) {
    let owner = unique("owner");
    let created = store
        .create_layout(NewLayout {
            owner: owner.clone(),
            title: "Ingest health".to_string(),
            visibility: Visibility::Personal,
            forked_from: None,
            time_range: Some(json!({ "from": "now-1h", "to": "now" })),
            panels: vec![panel("throughput"), panel("queue-depth")],
        })
        .await
        .unwrap();

    let read_back = store.layout(created.id).await.unwrap().unwrap();
    assert_eq!(read_back.owner, owner);
    assert_eq!(read_back.title, "Ingest health");
    assert_eq!(read_back.visibility, Visibility::Personal);
    assert_eq!(read_back.panels.len(), 2);
    assert_eq!(read_back.panels[0].reference(), "orebank/throughput");
    assert_eq!(read_back.time_range, created.time_range);

    store.delete_layout(created.id).await.unwrap();
}

async fn a_missing_layout_is_not_an_error(store: &dyn Store) {
    assert!(store.layout(Uuid::new_v4()).await.unwrap().is_none());
    assert!(
        !store.delete_layout(Uuid::new_v4()).await.unwrap(),
        "deleting something absent reports that there was nothing to delete"
    );
}

async fn layouts_are_listed_most_recently_changed_first(store: &dyn Store) {
    let owner = unique("collector");

    let first = store
        .create_layout(NewLayout::personal(owner.clone(), "First"))
        .await
        .unwrap();
    let second = store
        .create_layout(NewLayout::personal(owner.clone(), "Second"))
        .await
        .unwrap();

    // Touching the older one should move it to the front.
    let mut refreshed = store.layout(first.id).await.unwrap().unwrap();
    refreshed.title = "First, revisited".to_string();
    store.update_layout(&refreshed).await.unwrap();

    let listed = store.layouts_owned_by(&owner).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, first.id, "most recently changed comes first");
    assert_eq!(listed[1].id, second.id);

    assert!(
        store
            .layouts_owned_by(&unique("stranger"))
            .await
            .unwrap()
            .is_empty(),
        "one person's layouts are not another's"
    );

    store.delete_layout(first.id).await.unwrap();
    store.delete_layout(second.id).await.unwrap();
}

async fn only_published_layouts_are_in_the_gallery(store: &dyn Store) {
    let owner = unique("publisher");

    let private = store
        .create_layout(NewLayout::personal(owner.clone(), "Mine alone"))
        .await
        .unwrap();
    let shared = store
        .create_layout(NewLayout {
            visibility: Visibility::Published,
            ..NewLayout::personal(owner.clone(), "For everyone")
        })
        .await
        .unwrap();

    let gallery = store.published_layouts().await.unwrap();
    let listed: Vec<Uuid> = gallery.iter().map(|layout| layout.id).collect();
    assert!(listed.contains(&shared.id));
    assert!(!listed.contains(&private.id));

    // Publishing is one action, and takes effect immediately.
    let mut promoted = store.layout(private.id).await.unwrap().unwrap();
    promoted.visibility = Visibility::Published;
    store.update_layout(&promoted).await.unwrap();

    let gallery = store.published_layouts().await.unwrap();
    let listed: Vec<Uuid> = gallery.iter().map(|layout| layout.id).collect();
    assert!(listed.contains(&private.id));

    store.delete_layout(private.id).await.unwrap();
    store.delete_layout(shared.id).await.unwrap();
}

async fn updating_a_layout_replaces_its_panels(store: &dyn Store) {
    let owner = unique("arranger");
    let created = store
        .create_layout(NewLayout {
            panels: vec![panel("throughput"), panel("queue-depth")],
            ..NewLayout::personal(owner, "Rearranged")
        })
        .await
        .unwrap();

    let mut edited = store.layout(created.id).await.unwrap().unwrap();
    edited.panels.remove(0);
    edited.panels.push(panel("errors"));
    edited.panels[0].title_override = Some("Depth".to_string());
    edited.panels[0].kind_override = Some("sparkline".to_string());
    store.update_layout(&edited).await.unwrap();

    let read_back = store.layout(created.id).await.unwrap().unwrap();
    assert_eq!(read_back.panels.len(), 2);
    let references: Vec<String> = read_back
        .panels
        .iter()
        .map(PanelInstance::reference)
        .collect();
    assert!(references.contains(&"orebank/queue-depth".to_string()));
    assert!(references.contains(&"orebank/errors".to_string()));
    assert!(!references.contains(&"orebank/throughput".to_string()));

    let depth = read_back
        .panels
        .iter()
        .find(|p| p.panel_key == "queue-depth")
        .unwrap();
    assert_eq!(depth.title_override.as_deref(), Some("Depth"));
    assert_eq!(depth.kind_override.as_deref(), Some("sparkline"));

    store.delete_layout(created.id).await.unwrap();
}

async fn updating_a_layout_that_is_gone_is_an_error(store: &dyn Store) {
    let phantom = Layout {
        id: Uuid::new_v4(),
        owner: "nobody".to_string(),
        title: "Never existed".to_string(),
        visibility: Visibility::Personal,
        forked_from: None,
        time_range: None,
        panels: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(
        store.update_layout(&phantom).await.is_err(),
        "updating something absent is a real failure, not a silent create"
    );
}

async fn forking_gives_the_forker_their_own_copy(store: &dyn Store) {
    let author = unique("author");
    let reader = unique("reader");

    let original = store
        .create_layout(NewLayout {
            visibility: Visibility::Published,
            panels: vec![panel("throughput")],
            ..NewLayout::personal(author.clone(), "Worth copying")
        })
        .await
        .unwrap();

    let fork = store.fork_layout(original.id, &reader).await.unwrap();

    assert_eq!(fork.owner, reader, "a fork belongs to whoever forked it");
    assert_ne!(fork.id, original.id);
    assert_eq!(fork.forked_from, Some(original.id), "provenance is kept");
    assert_eq!(fork.title, original.title);
    assert_eq!(fork.panels.len(), 1);
    assert_ne!(
        fork.panels[0].id, original.panels[0].id,
        "panel instances are the fork's own"
    );
    assert_eq!(
        fork.visibility,
        Visibility::Personal,
        "publishing a fork is its new owner's decision, not inherited"
    );

    // The original is untouched, so the author keeps one source of truth.
    let original_again = store.layout(original.id).await.unwrap().unwrap();
    assert_eq!(original_again.owner, author);
    assert_eq!(original_again.panels.len(), 1);

    store.delete_layout(fork.id).await.unwrap();
    store.delete_layout(original.id).await.unwrap();
}

async fn forking_something_that_is_gone_is_an_error(store: &dyn Store) {
    assert!(store.fork_layout(Uuid::new_v4(), "anyone").await.is_err());
}

async fn deleting_a_layout_takes_its_panels_but_spares_its_forks(store: &dyn Store) {
    let author = unique("departing-author");
    let reader = unique("surviving-reader");

    let original = store
        .create_layout(NewLayout {
            panels: vec![panel("throughput")],
            ..NewLayout::personal(author, "Doomed")
        })
        .await
        .unwrap();
    let fork = store.fork_layout(original.id, &reader).await.unwrap();

    assert!(store.delete_layout(original.id).await.unwrap());
    assert!(store.layout(original.id).await.unwrap().is_none());

    let survivor = store
        .layout(fork.id)
        .await
        .unwrap()
        .expect("a fork outlives what it came from");
    assert_eq!(survivor.panels.len(), 1);
    assert_eq!(
        survivor.forked_from, None,
        "provenance is cleared once there is nothing left to point at"
    );

    store.delete_layout(fork.id).await.unwrap();
}

// -- Sessions -------------------------------------------------------------

/// A session, named the way the shell names them: never the cookie's value.
fn session(id: &str, subject: &str, lasting: chrono::Duration) -> Session {
    Session {
        id: id.to_string(),
        subject: subject.to_string(),
        name: Some(format!("{subject} of somewhere")),
        groups: vec!["platform-engineering".to_string()],
        created_at: Utc::now(),
        expires_at: Utc::now() + lasting,
    }
}

fn pending(state: &str, lasting: chrono::Duration) -> PendingLogin {
    PendingLogin {
        state: state.to_string(),
        nonce: format!("{state}-nonce"),
        code_verifier: format!("{state}-verifier"),
        redirect_to: "/s/somewhere".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + lasting,
    }
}

async fn a_session_round_trips(store: &dyn Store) {
    let id = format!("session-{}", Uuid::new_v4());
    store
        .create_session(session(&id, "ada", chrono::Duration::hours(1)))
        .await
        .unwrap();

    let read = store
        .session(&id)
        .await
        .unwrap()
        .expect("a session that has not expired is readable");

    assert_eq!(read.subject, "ada");
    assert_eq!(read.groups, vec!["platform-engineering".to_string()]);
    assert!(
        store
            .session("nothing-was-stored-here")
            .await
            .unwrap()
            .is_none(),
        "an unknown session is an ordinary answer, not an error"
    );

    store.end_session(&id).await.unwrap();
}

async fn an_expired_session_is_nobody(store: &dyn Store) {
    let id = format!("expired-{}", Uuid::new_v4());
    store
        .create_session(session(&id, "grace", -chrono::Duration::minutes(1)))
        .await
        .unwrap();

    // The row is still there — nothing has swept it — and it is still nobody.
    // A sweeper that has not run yet must never be the reason an expired
    // session still works.
    assert!(
        store.session(&id).await.unwrap().is_none(),
        "expiry is enforced on read, not only by the sweeper"
    );

    store.end_session(&id).await.unwrap();
}

async fn ending_a_session_ends_it(store: &dyn Store) {
    let id = format!("ending-{}", Uuid::new_v4());
    store
        .create_session(session(&id, "ada", chrono::Duration::hours(1)))
        .await
        .unwrap();

    assert!(store.end_session(&id).await.unwrap());
    assert!(
        store.session(&id).await.unwrap().is_none(),
        "signing out is what makes a copy of the cookie useless, which is the \
         whole reason sessions are rows rather than a signed cookie"
    );
    assert!(
        !store.end_session(&id).await.unwrap(),
        "ending a session twice says there was nothing to end"
    );
}

async fn a_sign_in_can_only_be_claimed_once(store: &dyn Store) {
    let state = format!("state-{}", Uuid::new_v4());
    store
        .begin_login(pending(&state, chrono::Duration::minutes(10)))
        .await
        .unwrap();

    let claimed = store
        .claim_login(&state)
        .await
        .unwrap()
        .expect("the sign-in that was started can be finished");
    assert_eq!(claimed.nonce, format!("{state}-nonce"));
    assert_eq!(claimed.redirect_to, "/s/somewhere");

    assert!(
        store.claim_login(&state).await.unwrap().is_none(),
        "a replayed callback finds nothing, which is what `state` is for"
    );
}

async fn an_abandoned_sign_in_cannot_be_finished(store: &dyn Store) {
    let state = format!("stale-{}", Uuid::new_v4());
    store
        .begin_login(pending(&state, -chrono::Duration::minutes(1)))
        .await
        .unwrap();

    assert!(
        store.claim_login(&state).await.unwrap().is_none(),
        "a sign-in somebody walked away from is not redeemable later"
    );
}

async fn sweeping_takes_only_what_is_over(store: &dyn Store) {
    let live = format!("live-{}", Uuid::new_v4());
    let dead = format!("dead-{}", Uuid::new_v4());

    store
        .create_session(session(&live, "ada", chrono::Duration::hours(1)))
        .await
        .unwrap();
    store
        .create_session(session(&dead, "grace", -chrono::Duration::hours(1)))
        .await
        .unwrap();
    store
        .begin_login(pending(&dead, -chrono::Duration::hours(1)))
        .await
        .unwrap();

    let swept = store.sweep_expired().await.unwrap();
    assert!(
        swept >= 2,
        "the expired session and the abandoned sign-in both go: {swept}"
    );

    assert!(
        store.session(&live).await.unwrap().is_some(),
        "a session in use is not housekeeping"
    );

    store.end_session(&live).await.unwrap();
}
