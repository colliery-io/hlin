//! The shell acting as its own continuous integration.
//!
//! No external service gates a platform's release, so these are the tests that
//! say the shell notices what a gate would have caught: a breaking change
//! without a major bump, a rollback that must not be flagged, and a flapping
//! deployment that must not raise a violation per flip (decision HLIN-A-0002).
//!
//! Everything here runs against a fake manifest client and the in-memory
//! store, so the registry's behaviour is tested without a network or a
//! database.

use std::sync::Arc;

use async_trait::async_trait;
use hlin::config::{AuthConfig, Config, PlatformConfig, Timings};
use hlin::identity::CredentialConfig;
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::store::{MemoryStore, Store};
use hlin_manifest::Manifest;
use tokio::sync::Mutex;

const PLATFORM: &str = "orebank";

/// A client that serves whatever the test last put in it.
struct Scripted {
    next: Mutex<Fetched>,
}

impl Scripted {
    fn new(fetched: Fetched) -> Arc<Self> {
        Arc::new(Self {
            next: Mutex::new(fetched),
        })
    }

    async fn serve(&self, fetched: Fetched) {
        *self.next.lock().await = fetched;
    }
}

/// A handle the registry can own, sharing one script with the test.
struct Handle(Arc<Scripted>);

#[async_trait]
impl ManifestClient for Handle {
    async fn fetch(&self, _base_url: &str) -> Fetched {
        self.0.next.lock().await.clone()
    }
}

fn manifest(version: &str, panels: &str) -> Manifest {
    hlin_manifest::parse_str(&format!(
        r#"{{
          "schema_version": 1,
          "contract_version": "{version}",
          "platform": {{ "id": "{PLATFORM}", "name": "Orebank" }},
          "panels": [{panels}],
          "health": "api/health"
        }}"#
    ))
    .expect("fixture parses")
}

const TWO_PANELS: &str = r#"
    { "key": "throughput", "title": "Throughput", "kind": "timeseries",
      "envelope": "series.v1", "data": "api/throughput" },
    { "key": "queue-depth", "title": "Queue depth", "kind": "stat",
      "envelope": "scalar.v1", "data": "api/queue-depth" }
"#;

const ONE_PANEL: &str = r#"
    { "key": "throughput", "title": "Throughput", "kind": "timeseries",
      "envelope": "series.v1", "data": "api/throughput" }
"#;

fn config(debounce: i32) -> Config {
    Config {
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::default(),
        timings: Timings {
            poll_seconds: 30,
            debounce_observations: debounce,
            upstream_timeout_seconds: 10,
            ..Default::default()
        },
        platforms: vec![PlatformConfig {
            id: PLATFORM.to_string(),
            base_url: "http://127.0.0.1:9999".to_string(),
            auth: CredentialConfig::HlinToken,
        }],
    }
}

fn build_registry(config: &Config, store: Arc<dyn Store>, client: Arc<Scripted>) -> Registry {
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    Registry::new(config, issuer, store, Arc::new(Handle(client))).expect("registry builds")
}

async fn poll(registry: &Registry) {
    registry.poll_one(PLATFORM, "http://127.0.0.1:9999").await;
}

// -- Discovery ------------------------------------------------------------

#[tokio::test]
async fn a_platform_is_known_once_it_has_been_asked() {
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store, client);

    // Before the first poll the shell knows the platform exists and nothing
    // more, which is the honest state rather than an empty list.
    {
        let views = registry.views().await;
        let view = views.get(PLATFORM).expect("configured");
        assert!(!view.reachable);
        assert!(view.accepted_panels().is_empty());
    }

    poll(&registry).await;

    let views = registry.views().await;
    let view = views.get(PLATFORM).expect("configured");
    assert!(view.reachable);
    assert_eq!(view.accepted_panels().len(), 2);
    assert!(view.panel("throughput").is_some());
}

#[tokio::test]
async fn a_platform_that_does_not_answer_keeps_the_panels_it_had() {
    // An existing surface must degrade rather than empty out, so the last
    // contract stays in place while the platform is away.
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store, client.clone());

    poll(&registry).await;
    client
        .serve(Fetched::Unreachable {
            reason: "connection refused".to_string(),
        })
        .await;
    poll(&registry).await;

    let views = registry.views().await;
    let view = views.get(PLATFORM).expect("configured");
    assert!(!view.reachable);
    assert_eq!(view.trouble.as_deref(), Some("connection refused"));
    assert_eq!(
        view.accepted_panels().len(),
        2,
        "the panels a viewer already has must not vanish because a platform is down"
    );
}

#[tokio::test]
async fn a_malformed_manifest_offers_no_panels_but_is_not_an_error() {
    let wrong_platform = hlin_manifest::parse_str(
        r#"{ "schema_version": 1, "contract_version": "1.0.0",
             "platform": { "id": "somebody-else", "name": "Other" },
             "health": "api/health" }"#,
    )
    .unwrap();

    let client = Scripted::new(Fetched::Document(Box::new(wrong_platform)));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store, client);

    poll(&registry).await;

    let views = registry.views().await;
    let view = views.get(PLATFORM).expect("configured");
    assert!(view.reachable, "it answered; it just said the wrong thing");
    assert!(view.trouble.is_some());
    assert!(view.accepted_panels().is_empty());
}

// -- Contract enforcement -------------------------------------------------

#[tokio::test]
async fn an_unchanged_contract_accumulates_observations() {
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store, client);

    poll(&registry).await;
    poll(&registry).await;
    poll(&registry).await;

    let views = registry.views().await;
    assert_eq!(views.get(PLATFORM).unwrap().consecutive_observations, 3);
}

#[tokio::test]
async fn a_breaking_change_is_judged_only_once_it_has_held() {
    // The bug this test exists to prevent: the baseline a change is judged
    // against must not move while the change is waiting to be believed, or the
    // second sighting compares against the first and finds nothing.
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store, client.clone());

    poll(&registry).await;
    let baseline = {
        let views = registry.views().await;
        views.get(PLATFORM).unwrap().contract_hash.clone()
    };
    assert!(baseline.is_some(), "the first contract is judged at once");

    // A panel disappears, with the version left alone.
    client
        .serve(Fetched::Document(Box::new(manifest("1.0.0", ONE_PANEL))))
        .await;

    poll(&registry).await;
    {
        let views = registry.views().await;
        let view = views.get(PLATFORM).unwrap();
        assert_eq!(view.consecutive_observations, 1);
        assert_eq!(
            view.contract_hash, baseline,
            "one sighting is not enough to judge a change"
        );
        assert_eq!(
            view.accepted_panels().len(),
            1,
            "but the shell still serves what the platform is actually serving"
        );
    }

    poll(&registry).await;
    {
        let views = registry.views().await;
        let view = views.get(PLATFORM).unwrap();
        assert_eq!(view.consecutive_observations, 2);
        assert_ne!(
            view.contract_hash, baseline,
            "the second sighting crosses the threshold and the change is judged"
        );
    }
}

#[tokio::test]
async fn a_flapping_deployment_never_crosses_the_threshold() {
    // Blue and green serving different revisions. Every poll sees a different
    // contract, so the count resets each time and nothing is ever judged.
    let blue = manifest("1.0.0", TWO_PANELS);
    let green = manifest("1.0.0", ONE_PANEL);

    let client = Scripted::new(Fetched::Document(Box::new(blue.clone())));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(3), store, client.clone());

    poll(&registry).await;
    let settled = {
        let views = registry.views().await;
        views.get(PLATFORM).unwrap().contract_hash.clone()
    };

    for round in 0..6 {
        let next = if round % 2 == 0 { &green } else { &blue };
        client
            .serve(Fetched::Document(Box::new(next.clone())))
            .await;
        poll(&registry).await;

        let views = registry.views().await;
        let view = views.get(PLATFORM).unwrap();
        assert_eq!(
            view.consecutive_observations, 1,
            "a contract that keeps changing is never seen twice in a row"
        );
        assert_eq!(
            view.contract_hash, settled,
            "so nothing is ever judged while it flaps"
        );
    }
}

#[tokio::test]
async fn contract_memory_survives_a_restart() {
    // Without this the shell forgets every contract on every deploy, and a
    // breaking change shipped across the restart window goes unnoticed.
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let config = config(1);

    {
        let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
        let registry = build_registry(&config, store.clone(), client);
        poll(&registry).await;
    }

    // A new shell, same store: what was known is loaded back.
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", ONE_PANEL))));
    let restarted = build_registry(&config, store.clone(), client);

    let restored = restarted.restore().await.expect("restores");
    assert_eq!(restored, 1);

    {
        let views = restarted.views().await;
        let view = views.get(PLATFORM).unwrap();
        assert_eq!(
            view.accepted_panels().len(),
            2,
            "the shell remembers what this platform last promised"
        );
    }

    // And the change shipped while it was down is judged on the first poll,
    // because there is a baseline to compare against.
    poll(&restarted).await;

    let views = restarted.views().await;
    let view = views.get(PLATFORM).unwrap();
    assert_eq!(view.accepted_panels().len(), 1);
    assert_eq!(view.consecutive_observations, 1);
}

#[tokio::test]
async fn an_additive_change_is_applied_without_complaint() {
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", ONE_PANEL))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(1), store, client.clone());

    poll(&registry).await;
    client
        .serve(Fetched::Document(Box::new(manifest("1.1.0", TWO_PANELS))))
        .await;
    poll(&registry).await;

    let views = registry.views().await;
    assert_eq!(views.get(PLATFORM).unwrap().accepted_panels().len(), 2);
}

#[tokio::test]
async fn a_rollback_is_applied_like_any_other_change() {
    // Rollbacks happen during incidents, which is exactly when the shell
    // matters most. The panels go back; nothing is flagged.
    let client = Scripted::new(Fetched::Document(Box::new(manifest("2.0.0", ONE_PANEL))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(1), store, client.clone());

    poll(&registry).await;
    client
        .serve(Fetched::Document(Box::new(manifest("1.9.0", TWO_PANELS))))
        .await;
    poll(&registry).await;

    let views = registry.views().await;
    let view = views.get(PLATFORM).unwrap();
    assert_eq!(view.accepted_panels().len(), 2);
    assert_eq!(
        view.manifest
            .as_ref()
            .map(|m| m.contract_version.to_string()),
        Some("1.9.0".to_string())
    );
}

#[tokio::test]
async fn a_violation_is_written_down_and_survives_a_restart() {
    // HLIN-A-0002 calls a violation "an operator signal", and until now the
    // signal was one `tracing::error!` at the moment of classification. An
    // operator who was not tailing the log then could not answer the question
    // the mechanism exists to answer: has this platform ever done this?
    let client = Scripted::new(Fetched::Document(Box::new(manifest("1.0.0", TWO_PANELS))));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = build_registry(&config(2), store.clone(), client.clone());

    poll(&registry).await;
    assert!(
        registry
            .views()
            .await
            .get(PLATFORM)
            .unwrap()
            .last_violation
            .is_none(),
        "a platform that has behaved has nothing recorded against it"
    );

    // A panel disappears with the version left alone, twice, so the debounce
    // believes it.
    client
        .serve(Fetched::Document(Box::new(manifest("1.0.0", ONE_PANEL))))
        .await;
    poll(&registry).await;
    poll(&registry).await;

    let recorded = {
        let views = registry.views().await;
        views.get(PLATFORM).unwrap().last_violation.clone()
    };
    let recorded = recorded.expect("the violation is on the live view");
    assert_eq!(recorded.declared, "1.0.0");
    assert_eq!(recorded.expected_major, 2);
    assert_eq!(recorded.seen, 1);
    assert!(
        recorded
            .changes
            .iter()
            .any(|change| change.contains("PanelRemoved")),
        "what broke is recorded in words, not left to be reconstructed: {:?}",
        recorded.changes
    );

    // The restart window is exactly when a breaking change is most likely to go
    // unwatched, so the record has to outlive the process that made it.
    let restarted = build_registry(&config(2), store, client);
    let restored = restarted
        .restore()
        .await
        .expect("contract memory is restored");
    assert_eq!(restored, 1);

    let after = {
        let views = restarted.views().await;
        views.get(PLATFORM).unwrap().last_violation.clone()
    };
    assert_eq!(
        after.expect("the violation survived the restart").declared,
        "1.0.0",
        "a violation that only lived in the log would be gone here"
    );
}
