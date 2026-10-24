//! The admin role (HLIN-A-0016), over the shell's own HTTP surface.
//!
//! Every route is driven as four signed-in people and as an anonymous
//! visitor:
//!
//! - `ada` owns the layouts and is not an admin;
//! - `grace` is another signed-in person, not an admin, in a group that is
//!   not the admin group;
//! - `root` is an admin because `[admin] principals` names it;
//! - `sam` is an admin by the strategy: the admin group in the groups header
//!   under `trusted-header`, a value of the admin claim under `oidc`.
//!
//! The same claims run under `trusted-header` and `oidc`, so a check that
//! only works for one strategy fails here. `dev` and `anonymous` have their
//! own cases at the end.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hlin::config::{AdminConfig, AuthConfig, Config, CookieConfig, OidcConfig, Timings};
use hlin::manifest_client::{Fetched, ManifestClient};
use hlin::registry::Registry;
use hlin::server::{AppState, router};
use hlin::store::{Layout, MemoryStore, NewLayout, Store, Visibility};
use serde_json::{Value, json};
use tower::ServiceExt;

/// No platforms are configured, so nothing is ever fetched.
struct Nowhere;

#[async_trait]
impl ManifestClient for Nowhere {
    async fn fetch(&self, base_url: &str) -> Fetched {
        Fetched::Unreachable {
            reason: format!("no platform at {base_url} in these tests"),
        }
    }
}

const OWNER: &str = "ada";
const OTHER: &str = "grace";
const BY_CONFIG: &str = "root";
const BY_CLAIM: &str = "sam";
const PEOPLE: [&str; 4] = [OWNER, OTHER, BY_CONFIG, BY_CLAIM];

fn config(auth: AuthConfig) -> Config {
    Config {
        public_url: None,
        modules: Default::default(),
        compression: Default::default(),
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        ca_bundle: None,
        brand: Default::default(),
        database_url: None,
        database_url_env: None,
        admin: AdminConfig {
            principals: vec![BY_CONFIG.to_string()],
        },
        frontend: "unused".into(),
        auth,
        timings: Timings::default(),
        platforms: vec![],
    }
}

fn trusted_header() -> AuthConfig {
    AuthConfig::TrustedHeader {
        header: "x-forwarded-user".to_string(),
        groups_header: Some("x-forwarded-groups".to_string()),
        name_header: None,
        admin_groups: vec!["hlin-admins".to_string()],
        acknowledge_proxy_required: true,
    }
}

fn oidc() -> AuthConfig {
    AuthConfig::Oidc(OidcConfig {
        issuer: "https://login.example.com".to_string(),
        client_id: "hlin".to_string(),
        client_secret_env: "HLIN_TEST_OIDC_SECRET".to_string(),
        public_url: "https://hlin.example.com".to_string(),
        scopes: vec!["openid".to_string()],
        groups_claim: "groups".to_string(),
        // A claim of its own, so the test proves the admin check reads it
        // and not the groups that go to platforms.
        admin_claim: Some("roles".to_string()),
        admin_values: vec!["hlin-admin".to_string()],
        session_hours: 12,
        cookie: Default::default(),
    })
}

fn anonymous() -> AuthConfig {
    AuthConfig::Anonymous {
        cookie: CookieConfig {
            name: "hlin_visitor".to_string(),
            secure: false,
            same_site: "Lax".to_string(),
        },
        name: None,
    }
}

/// A shell, its store, and how to send a request as each person.
struct Shell {
    app: axum::Router,
    store: Arc<dyn Store>,
    /// Headers that make a request one person's.
    as_person: HashMap<&'static str, Vec<(&'static str, String)>>,
}

async fn shell(auth: AuthConfig) -> Shell {
    let config = Arc::new(config(auth));
    let issuer = Arc::new(hlin_identity::Issuer::generate("hlin"));
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let registry = Arc::new(
        Registry::new(&config, issuer.clone(), store.clone(), Arc::new(Nowhere))
            .expect("registry builds"),
    );

    let mut as_person = HashMap::new();
    match &config.auth {
        AuthConfig::TrustedHeader { .. } => {
            for person in PEOPLE {
                let groups = if person == BY_CLAIM {
                    "oncall, hlin-admins"
                } else {
                    "oncall"
                };
                as_person.insert(
                    person,
                    vec![
                        ("x-forwarded-user", person.to_string()),
                        ("x-forwarded-groups", groups.to_string()),
                    ],
                );
            }
        }
        AuthConfig::Oidc(_) => {
            for person in PEOPLE {
                let value = hlin::auth::unguessable();
                // Everyone is in a group named like the admin value, so a
                // check that read `groups` instead of the admin claim would
                // make everyone an admin and fail below.
                let admin_claim = if person == BY_CLAIM {
                    vec!["hlin-admin".to_string()]
                } else {
                    vec!["viewer".to_string()]
                };
                store
                    .create_session(hlin::store::Session {
                        id: hlin::auth::fingerprint(&value),
                        subject: person.to_string(),
                        name: None,
                        email: None,
                        groups: vec!["hlin-admin".to_string()],
                        admin_claim,
                        created_at: chrono::Utc::now(),
                        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                    })
                    .await
                    .expect("the store accepts a session");
                as_person.insert(person, vec![("cookie", format!("hlin_session={value}"))]);
            }
        }
        _ => {}
    }

    let state = AppState {
        config: config.clone(),
        registry,
        issuer,
        surfaces: Arc::new(hlin::surfaces::Surfaces::new()),
        store: store.clone(),
        client: reqwest::Client::new(),
        stream_client: reqwest::Client::new(),
        proxy_client: hlin::clients::Clients::plain().proxying,
        compressed: Default::default(),
        streams: Arc::new(hlin::stream::streams::Streams::new()),
    };

    Shell {
        app: router(state),
        store,
        as_person,
    }
}

impl Shell {
    /// A request as one person, or as nobody in particular with `None`.
    async fn call(
        &self,
        who: Option<&str>,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(who) = who {
            for (name, value) in self.as_person.get(who).cloned().unwrap_or_default() {
                builder = builder.header(name, value);
            }
        }
        let request = match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => builder.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("a readable body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn layout(&self, owner: &str, title: &str, visibility: Visibility) -> Layout {
        self.store
            .create_layout(NewLayout {
                visibility,
                ..NewLayout::personal(owner, title)
            })
            .await
            .expect("the store accepts a layout")
    }

    async fn is_stored(&self, layout: &Layout) -> bool {
        self.store
            .layout(layout.id)
            .await
            .expect("a store")
            .is_some()
    }
}

fn document(title: &str, visibility: &str) -> Value {
    json!({ "title": title, "visibility": visibility, "items": [] })
}

/// Run one claim under both strategies that can name an admin by group or
/// claim.
macro_rules! under_both {
    ($name:ident, $claim:ident) => {
        mod $name {
            #[tokio::test]
            async fn under_trusted_header() {
                super::$claim(super::shell(super::trusted_header()).await).await;
            }

            #[tokio::test]
            async fn under_oidc() {
                super::$claim(super::shell(super::oidc()).await).await;
            }
        }
    };
}

// -- /api/config ----------------------------------------------------------

async fn the_config_says_who_is_an_admin(shell: Shell) {
    for (person, admin) in [
        (OWNER, false),
        (OTHER, false),
        (BY_CONFIG, true),
        (BY_CLAIM, true),
    ] {
        let (status, body) = shell.call(Some(person), "GET", "/api/config", None).await;
        assert_eq!(status, StatusCode::OK, "{person}");
        assert_eq!(body["admin"], json!(admin), "{person}");

        let parsed: hlin_stream::layout::ClientConfig =
            serde_json::from_value(body).expect("a browser can read it");
        assert_eq!(parsed.admin, admin, "{person}");
    }
}
under_both!(config_admin, the_config_says_who_is_an_admin);

// -- Visibility -----------------------------------------------------------

async fn only_an_admin_makes_a_layout_public(shell: Shell) {
    // Anyone signed in may publish.
    let mine = shell.layout(OWNER, "Mine", Visibility::Personal).await;
    let path = format!("/api/layouts/{}", mine.id);
    let (status, _) = shell
        .call(
            Some(OWNER),
            "PUT",
            &path,
            Some(document("Mine", "published")),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "publishing is open to anyone signed in"
    );

    // Not public.
    let (status, body) = shell
        .call(Some(OWNER), "PUT", &path, Some(document("Mine", "public")))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body["error"].as_str().unwrap_or("").contains("admin"),
        "{body}"
    );
    let stored = shell.store.layout(mine.id).await.unwrap().unwrap();
    assert_eq!(stored.visibility, Visibility::Published, "nothing changed");

    // Another person cannot write it at all, admin or not: PUT is the owner's.
    for person in [OTHER, BY_CONFIG, BY_CLAIM] {
        let (status, _) = shell
            .call(Some(person), "PUT", &path, Some(document("Mine", "public")))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{person}");
    }

    // An admin makes their own layout public, by config or by claim.
    for admin in [BY_CONFIG, BY_CLAIM] {
        let theirs = shell
            .layout(admin, "For anybody", Visibility::Personal)
            .await;
        let (status, body) = shell
            .call(
                Some(admin),
                "PUT",
                &format!("/api/layouts/{}", theirs.id),
                Some(document("For anybody", "public")),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{admin}: {body}");
        assert_eq!(body["visibility"], json!("public"), "{admin}");
    }

    // A layout that is already public stays public when its owner edits it,
    // admin or not, and its owner may take it back.
    let kept = shell.layout(OWNER, "Kept", Visibility::Public).await;
    let path = format!("/api/layouts/{}", kept.id);
    let (status, _) = shell
        .call(
            Some(OWNER),
            "PUT",
            &path,
            Some(document("Kept, renamed", "public")),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = shell
        .call(
            Some(OWNER),
            "PUT",
            &path,
            Some(document("Kept", "personal")),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["visibility"], json!("personal"));
}
under_both!(public_visibility, only_an_admin_makes_a_layout_public);

async fn signed_in_people_see_published_and_public_layouts(shell: Shell) {
    let published = shell
        .layout(OWNER, "Published", Visibility::Published)
        .await;
    let public = shell.layout(OWNER, "Public", Visibility::Public).await;
    let personal = shell.layout(OWNER, "Personal", Visibility::Personal).await;

    for person in PEOPLE {
        for layout in [&published, &public] {
            let (status, body) = shell
                .call(
                    Some(person),
                    "GET",
                    &format!("/api/layouts/{}", layout.id),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{person} reads {}", layout.title);
            assert_eq!(body["editable"], json!(person == OWNER), "{person}");
        }

        let (status, _) = shell
            .call(
                Some(person),
                "GET",
                &format!("/api/layouts/{}", personal.id),
                None,
            )
            .await;
        let expected = if person == OWNER {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        assert_eq!(
            status, expected,
            "{person}: a personal layout is the owner's"
        );
    }
}
under_both!(
    reading_layouts,
    signed_in_people_see_published_and_public_layouts
);

// -- The org default ------------------------------------------------------

async fn the_org_default_is_set_and_cleared_by_an_admin_only(shell: Shell) {
    let published = shell.layout(OWNER, "Shared", Visibility::Published).await;
    let personal = shell.layout(OWNER, "Private", Visibility::Personal).await;
    let set_to = |id: &uuid::Uuid| Some(json!({ "id": id.to_string() }));

    // Nothing set yet.
    for person in PEOPLE {
        let (status, _) = shell
            .call(Some(person), "GET", "/api/layouts/default", None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{person}");
    }

    // Not an admin: refused, even for a layout of their own.
    for person in [OWNER, OTHER] {
        let (status, body) = shell
            .call(
                Some(person),
                "PUT",
                "/api/layouts/default",
                set_to(&published.id),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{person}");
        assert!(
            body["error"].as_str().unwrap_or("").contains("admin"),
            "{body}"
        );
    }
    assert_eq!(shell.store.org_default().await.unwrap(), None);

    // An admin may not make a personal layout the default.
    let (status, body) = shell
        .call(
            Some(BY_CONFIG),
            "PUT",
            "/api/layouts/default",
            set_to(&personal.id),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"].as_str().unwrap_or("").contains("published"),
        "{body}"
    );

    // Nor one that does not exist.
    let (status, _) = shell
        .call(
            Some(BY_CONFIG),
            "PUT",
            "/api/layouts/default",
            set_to(&uuid::Uuid::new_v4()),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = shell
        .call(
            Some(BY_CONFIG),
            "PUT",
            "/api/layouts/default",
            Some(json!({ "id": "not-a-layout" })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // An admin by config sets it.
    let (status, body) = shell
        .call(
            Some(BY_CONFIG),
            "PUT",
            "/api/layouts/default",
            set_to(&published.id),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], json!(published.id.to_string()));

    // Anyone signed in reads it.
    for person in PEOPLE {
        let (status, body) = shell
            .call(Some(person), "GET", "/api/layouts/default", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{person}");
        assert_eq!(body["id"], json!(published.id.to_string()), "{person}");
    }

    // Not an admin: cannot clear it.
    for person in [OWNER, OTHER] {
        let (status, _) = shell
            .call(Some(person), "DELETE", "/api/layouts/default", None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{person}");
    }
    assert_eq!(shell.store.org_default().await.unwrap(), Some(published.id));

    // An admin by claim clears it.
    let (status, _) = shell
        .call(Some(BY_CLAIM), "DELETE", "/api/layouts/default", None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = shell
        .call(Some(OWNER), "GET", "/api/layouts/default", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And sets it again, to a public layout this time.
    let public = shell
        .layout(BY_CLAIM, "For anybody", Visibility::Public)
        .await;
    let (status, _) = shell
        .call(
            Some(BY_CLAIM),
            "PUT",
            "/api/layouts/default",
            set_to(&public.id),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shell.store.org_default().await.unwrap(), Some(public.id));
}
under_both!(
    org_default,
    the_org_default_is_set_and_cleared_by_an_admin_only
);

async fn a_default_made_personal_again_is_not_served(shell: Shell) {
    let shared = shell.layout(OWNER, "Shared", Visibility::Published).await;
    shell.store.set_org_default(Some(shared.id)).await.unwrap();

    let (status, _) = shell
        .call(
            Some(OWNER),
            "PUT",
            &format!("/api/layouts/{}", shared.id),
            Some(document("Shared", "personal")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the owner may take it back");

    let (status, _) = shell
        .call(Some(OTHER), "GET", "/api/layouts/default", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "it is no longer shared");
}
under_both!(
    default_made_personal,
    a_default_made_personal_again_is_not_served
);

// -- Deleting -------------------------------------------------------------

async fn an_admin_deletes_what_someone_else_shared(shell: Shell) {
    let delete = |layout: &Layout| format!("/api/layouts/{}", layout.id);

    // A personal layout is its owner's alone, admin or not.
    let personal = shell.layout(OWNER, "Private", Visibility::Personal).await;
    for person in [OTHER, BY_CONFIG, BY_CLAIM] {
        let (status, _) = shell
            .call(Some(person), "DELETE", &delete(&personal), None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{person}");
    }
    assert!(shell.is_stored(&personal).await);

    // A published or public layout: another person who is not an admin
    // cannot delete it.
    let published = shell
        .layout(OWNER, "Published", Visibility::Published)
        .await;
    let public = shell.layout(OWNER, "Public", Visibility::Public).await;
    for layout in [&published, &public] {
        let (status, _) = shell
            .call(Some(OTHER), "DELETE", &delete(layout), None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{}", layout.title);
        assert!(shell.is_stored(layout).await);
    }

    // An admin can: by config for the published one, by claim for the public.
    let (status, _) = shell
        .call(Some(BY_CONFIG), "DELETE", &delete(&published), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!shell.is_stored(&published).await);

    let (status, _) = shell
        .call(Some(BY_CLAIM), "DELETE", &delete(&public), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!shell.is_stored(&public).await);

    // The owner deletes their own, whatever its visibility.
    let (status, _) = shell
        .call(Some(OWNER), "DELETE", &delete(&personal), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let shared = shell.layout(OWNER, "Shared", Visibility::Published).await;
    let (status, _) = shell
        .call(Some(OWNER), "DELETE", &delete(&shared), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Deleting the org default clears it.
    let default = shell.layout(OWNER, "Default", Visibility::Published).await;
    shell.store.set_org_default(Some(default.id)).await.unwrap();
    let (status, _) = shell
        .call(Some(BY_CONFIG), "DELETE", &delete(&default), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = shell
        .call(Some(OTHER), "GET", "/api/layouts/default", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
under_both!(deleting, an_admin_deletes_what_someone_else_shared);

// -- Home -----------------------------------------------------------------

async fn home_is_your_own_then_the_default_then_a_new_one(shell: Shell) {
    // No layout and no default: one is created, as before.
    let (status, body) = shell
        .call(Some(OTHER), "GET", "/api/layouts/home", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["owner"], json!(OTHER));
    assert_eq!(body["editable"], json!(true));
    let created = body["id"].clone();
    shell
        .store
        .delete_layout(uuid::Uuid::parse_str(created.as_str().unwrap()).unwrap())
        .await
        .unwrap();

    // No layout of their own, a default set: the default, read-only.
    let default = shell.layout(OWNER, "Default", Visibility::Published).await;
    shell.store.set_org_default(Some(default.id)).await.unwrap();
    let (status, body) = shell
        .call(Some(OTHER), "GET", "/api/layouts/home", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!(default.id.to_string()));
    assert_eq!(body["editable"], json!(false), "they fork it to edit");
    assert!(
        shell
            .store
            .layouts_owned_by(OTHER)
            .await
            .unwrap()
            .is_empty(),
        "and nothing was created for them"
    );

    // A layout of their own wins over the default.
    let own = shell.layout(OTHER, "Mine", Visibility::Personal).await;
    let (_, body) = shell
        .call(Some(OTHER), "GET", "/api/layouts/home", None)
        .await;
    assert_eq!(body["id"], json!(own.id.to_string()));

    // The owner of the default gets it as their own most recent layout.
    let (_, body) = shell
        .call(Some(OWNER), "GET", "/api/layouts/home", None)
        .await;
    assert_eq!(body["id"], json!(default.id.to_string()));
    assert_eq!(body["editable"], json!(true));
}
under_both!(home, home_is_your_own_then_the_default_then_a_new_one);

// -- dev ------------------------------------------------------------------

/// `dev` has no group or claim: its one principal is an admin only when
/// `[admin] principals` names it.
#[tokio::test]
async fn dev_is_an_admin_only_when_listed() {
    let not_listed = shell(AuthConfig::default()).await;
    let (_, body) = not_listed.call(None, "GET", "/api/config", None).await;
    assert_eq!(body["admin"], json!(false));
    let (status, _) = not_listed
        .call(
            None,
            "PUT",
            "/api/layouts/default",
            Some(json!({ "id": uuid::Uuid::new_v4().to_string() })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let listed = shell(AuthConfig::Dev {
        sub: BY_CONFIG.to_string(),
        name: None,
        groups: vec!["hlin-admins".to_string()],
    })
    .await;
    let (_, body) = listed.call(None, "GET", "/api/config", None).await;
    assert_eq!(body["admin"], json!(true));

    let shared = listed.layout(OWNER, "Shared", Visibility::Published).await;
    let (status, _) = listed
        .call(
            None,
            "PUT",
            "/api/layouts/default",
            Some(json!({ "id": shared.id.to_string() })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // A group in `dev` names no admin.
    let grouped = shell(AuthConfig::Dev {
        sub: OTHER.to_string(),
        name: None,
        groups: vec!["hlin-admins".to_string()],
    })
    .await;
    let (_, body) = grouped.call(None, "GET", "/api/config", None).await;
    assert_eq!(body["admin"], json!(false));
}

// -- anonymous ------------------------------------------------------------

/// Nobody is an admin where nobody signs in, and every admin route is a write
/// the shell refuses. `[admin] principals` names `root` here too, and changes
/// nothing.
#[tokio::test]
async fn an_anonymous_visitor_is_never_an_admin() {
    let shell = shell(anonymous()).await;
    let published = shell
        .layout(OWNER, "Published", Visibility::Published)
        .await;
    let public = shell.layout(OWNER, "Public", Visibility::Public).await;

    let (status, body) = shell.call(None, "GET", "/api/config", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["admin"], json!(false));
    assert_eq!(body["read_only"], json!(true));

    let writes = [
        (
            "PUT",
            "/api/layouts/default".to_string(),
            Some(json!({ "id": public.id.to_string() })),
        ),
        ("DELETE", "/api/layouts/default".to_string(), None),
        ("DELETE", format!("/api/layouts/{}", public.id), None),
        ("DELETE", format!("/api/layouts/{}", published.id), None),
        (
            "PUT",
            format!("/api/layouts/{}", public.id),
            Some(document("Public", "public")),
        ),
    ];
    for (method, path, body) in writes {
        let (status, answer) = shell.call(None, method, &path, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(answer["error"], json!("read_only"), "{method} {path}");
    }
    assert!(shell.is_stored(&public).await);
    assert!(shell.is_stored(&published).await);
}

/// An anonymous shell shows only public layouts: by link, as the default,
/// and on its stream.
#[tokio::test]
async fn an_anonymous_visitor_sees_only_what_is_public() {
    let shell = shell(anonymous()).await;
    let personal = shell.layout(OWNER, "Personal", Visibility::Personal).await;
    let published = shell
        .layout(OWNER, "Published", Visibility::Published)
        .await;
    let public = shell.layout(OWNER, "Public", Visibility::Public).await;

    for layout in [&personal, &published] {
        let (status, _) = shell
            .call(None, "GET", &format!("/api/layouts/{}", layout.id), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{}", layout.title);

        let (status, _) = shell
            .call(None, "GET", &format!("/api/stream/{}", layout.id), None)
            .await;
        assert_ne!(status, StatusCode::OK, "no stream for {}", layout.title);
    }
    let (status, body) = shell
        .call(None, "GET", &format!("/api/layouts/{}", public.id), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["editable"], json!(false));

    // A default that is only published is not shown, nor used by home.
    shell
        .store
        .set_org_default(Some(published.id))
        .await
        .unwrap();
    let (status, _) = shell.call(None, "GET", "/api/layouts/default", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = shell.call(None, "GET", "/api/layouts/home", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["id"],
        json!(public.id.to_string()),
        "the most recent public layout instead"
    );

    // A public default is shown, and home opens it ahead of a newer public
    // layout.
    let default = shell.layout(OWNER, "Default", Visibility::Public).await;
    shell.store.set_org_default(Some(default.id)).await.unwrap();
    let newer = shell.layout(OWNER, "Newer", Visibility::Public).await;
    let (status, body) = shell.call(None, "GET", "/api/layouts/default", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!(default.id.to_string()));
    let (_, body) = shell.call(None, "GET", "/api/layouts/home", None).await;
    assert_eq!(body["id"], json!(default.id.to_string()));
    assert_ne!(body["id"], json!(newer.id.to_string()));
}

/// Nothing public: home says why, plainly.
#[tokio::test]
async fn an_anonymous_shell_with_nothing_public_says_so() {
    let shell = shell(anonymous()).await;
    shell
        .layout(OWNER, "Published", Visibility::Published)
        .await;

    let (status, body) = shell.call(None, "GET", "/api/layouts/home", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let reason = body["error"].as_str().unwrap_or("");
    assert!(reason.contains("public"), "{body}");
    assert!(reason.contains("admin"), "{body}");
}

// -- Configuration --------------------------------------------------------

#[test]
fn admin_settings_that_cannot_work_refuse_to_start() {
    let mut no_groups_header = config(trusted_header());
    if let AuthConfig::TrustedHeader { groups_header, .. } = &mut no_groups_header.auth {
        *groups_header = None;
    }
    let refused = no_groups_header.check().expect_err("refused");
    assert!(refused.to_string().contains("groups_header"), "{refused}");

    // SAFETY: set before the check reads it; no other test reads this name.
    unsafe { std::env::set_var("HLIN_TEST_ADMIN_OIDC_SECRET", "unused") };
    let mut no_values = config(oidc());
    if let AuthConfig::Oidc(oidc) = &mut no_values.auth {
        oidc.client_secret_env = "HLIN_TEST_ADMIN_OIDC_SECRET".to_string();
        oidc.admin_values.clear();
    }
    let refused = no_values.check().expect_err("refused");
    assert!(refused.to_string().contains("admin_values"), "{refused}");
}

#[test]
fn admin_settings_read_from_toml() {
    let text = r#"
        [admin]
        principals = ["root"]

        [auth]
        strategy = "oidc"
        issuer = "https://login.example.com"
        client_id = "hlin"
        client_secret_env = "HLIN_OIDC_SECRET"
        public_url = "https://hlin.example.com"
        admin_claim = "roles"
        admin_values = ["hlin-admin"]
    "#;
    let config: Config = toml::from_str(text).expect("parses");
    assert_eq!(config.admin.principals, vec!["root".to_string()]);
    let AuthConfig::Oidc(oidc) = &config.auth else {
        panic!("oidc");
    };
    assert_eq!(oidc.admin_claim(), "roles");
    assert_eq!(oidc.admin_values, vec!["hlin-admin".to_string()]);

    let text = r#"
        [auth]
        strategy = "trusted-header"
        groups_header = "x-forwarded-groups"
        admin_groups = ["hlin-admins"]
        acknowledge_proxy_required = true
    "#;
    let config: Config = toml::from_str(text).expect("parses");
    assert!(
        config.admin.principals.is_empty(),
        "no [admin] is no admins"
    );
    let AuthConfig::TrustedHeader { admin_groups, .. } = &config.auth else {
        panic!("trusted-header");
    };
    assert_eq!(admin_groups, &vec!["hlin-admins".to_string()]);
}
