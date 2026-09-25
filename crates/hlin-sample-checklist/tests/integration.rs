//! The checklist, driven as the shell drives it.
//!
//! Every request here carries a real token minted by an `Issuer`, one person
//! at a time, read tokens on reads and tokens bound to the request on writes,
//! exactly as the shell's request proxy mints them ([[HLIN-S-0007]]). The
//! platform is checked for what it decides from them, not for how it parses
//! them: that is `hlin-identity`'s to test.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use hlin_identity::extract::IdentityState;
use hlin_identity::{BoundRequest, IDENTITY_HEADER, Issuer, Principal, Verifier};
use hlin_manifest::envelope::Envelope;
use hlin_manifest::{parse_envelope, validate};
use hlin_sample_checklist::lists::Lists;
use hlin_sample_checklist::routes::{IDEMPOTENCY_KEY, REPLAYED};
use hlin_sample_checklist::{App, changes, manifest, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const PLATFORM: &str = "checklist";

fn alice() -> Principal {
    person("u-alice", "Alice", "alice@example.com")
}

fn bob() -> Principal {
    person("u-bob", "Bob", "bob@example.com")
}

fn carol() -> Principal {
    person("u-carol", "Carol", "carol@elsewhere.org")
}

fn person(sub: &str, name: &str, email: &str) -> Principal {
    Principal {
        sub: sub.to_string(),
        name: Some(name.to_string()),
        email: Some(email.to_string()),
        groups: vec![],
    }
}

/// Which token a request carries.
#[derive(Clone, Copy)]
enum Token<'a> {
    /// What the shell sends: unbound on a read, bound to the request on a
    /// write.
    AsTheShellWould,
    /// An ordinary read token, whatever the method.
    Unbound,
    /// A token bound to some other request.
    BoundTo(&'a str, &'a str),
    /// A correctly bound token, for a different platform.
    ForAnotherPlatform,
    /// Nothing at all.
    Absent,
}

struct Platform {
    issuer: Issuer,
    app: App,
    router: Router,
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Platform {
    fn new() -> Self {
        let issuer = Issuer::generate("hlin");
        let identity = IdentityState {
            verifier: Arc::new(Verifier::with_keys("hlin", issuer.jwks())),
            audience: PLATFORM.to_string(),
            #[cfg(feature = "dev-identity")]
            development_principal: alice(),
        };
        let app = App::new(PLATFORM, identity, Lists::seeded());
        Self {
            issuer,
            router: router(app.clone()),
            app,
        }
    }

    fn token(&self, who: &Principal, method: &Method, path: &str, token: Token) -> Option<String> {
        let bare = path.split('?').next().expect("a path");
        let bound = |method: &str, path: &str, audience: &str| {
            let request = BoundRequest::new(method, path).expect("a bindable request");
            self.issuer
                .mint_bound(who, audience, &request)
                .expect("mints")
        };
        match token {
            Token::AsTheShellWould if *method == Method::GET => {
                Some(self.issuer.mint(who, PLATFORM).expect("mints"))
            }
            Token::AsTheShellWould => Some(bound(method.as_str(), bare, PLATFORM)),
            Token::Unbound => Some(self.issuer.mint(who, PLATFORM).expect("mints")),
            Token::BoundTo(method, path) => Some(bound(method, path, PLATFORM)),
            Token::ForAnotherPlatform => Some(bound(method.as_str(), bare, "feed")),
            Token::Absent => None,
        }
    }

    async fn send(
        &self,
        who: &Principal,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
        token: Token<'_>,
    ) -> Answer {
        let mut request = Request::builder().method(method.clone()).uri(path);
        if let Some(token) = self.token(who, &method, path, token) {
            request = request.header(IDENTITY_HEADER, token);
        }
        if let Some(key) = key {
            request = request.header(IDEMPOTENCY_KEY, key);
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Answer {
            status,
            headers,
            body,
        }
    }

    async fn get(&self, who: &Principal, path: &str) -> Answer {
        self.send(who, Method::GET, path, None, None, Token::AsTheShellWould)
            .await
    }

    async fn write(
        &self,
        who: &Principal,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> Answer {
        self.send(who, method, path, body, key, Token::AsTheShellWould)
            .await
    }

    async fn add(&self, who: &Principal, list: &str, text: &str) -> Answer {
        self.write(
            who,
            Method::POST,
            &format!("/api/lists/{list}/items"),
            Some(json!({ "text": text })),
            None,
        )
        .await
    }

    fn item(&self, list: &str, id: &str) -> Option<hlin_sample_checklist::lists::Item> {
        let alice = hlin_sample_checklist::lists::Caller {
            id: "anyone".into(),
            name: "anyone".into(),
            email: Some(if list == "carol" {
                "carol@elsewhere.org".into()
            } else {
                "alice@example.com".into()
            }),
        };
        self.app
            .lists()
            .read(&alice, list)
            .ok()?
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }
}

fn id_of(answer: &Answer) -> String {
    answer.body["id"]
        .as_str()
        .unwrap_or_else(|| panic!("an item, got {} {}", answer.status, answer.body))
        .to_string()
}

// -- The manifest is one the shell accepts -----------------------------------

#[test]
fn the_manifest_is_valid_and_every_route_prefix_is_usable() {
    let document = manifest::build(PLATFORM);
    let checked = validate(&document, PLATFORM);

    assert!(checked.is_document_valid(), "{:?}", checked.document);
    assert!(checked.rejected().is_empty(), "{:?}", checked.rejected());
    assert!(
        checked.unusable_routes.is_empty(),
        "{:?}",
        checked.unusable_routes
    );
    assert_eq!(checked.accepted_keys(), vec![changes::ITEMS]);
}

#[test]
fn the_manifest_round_trips_through_the_contract_crate() {
    let document = manifest::build(PLATFORM);
    let json = serde_json::to_string(&document).expect("serialises");
    assert_eq!(document, hlin_manifest::parse_str(&json).expect("parses"));
}

#[test]
fn the_items_panel_is_a_pushed_table_with_a_list_picker() {
    let document = manifest::build(PLATFORM);
    let panel = document.panel(changes::ITEMS).expect("the panel");

    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(panel.pushed, "a list changes when somebody changes it");
    assert!(document.events.is_some(), "pushed needs a stream");
    assert!(panel.ui.is_none(), "the module is HLIN-T-0074's");

    let select = panel
        .params
        .iter()
        .find(|param| param.param == "select")
        .expect("a select");
    assert_eq!(select.config["id"], changes::LIST_PARAM);
    assert_eq!(select.config["options"], manifest::LISTS_OPTIONS);

    let routes = document.routes.as_ref().expect("routes");
    assert_eq!(routes.read, vec!["/api/"]);
    assert_eq!(routes.write, vec!["/api/"]);
}

#[tokio::test]
async fn the_manifest_is_served_to_anyone() {
    let platform = Platform::new();
    let answer = platform
        .send(
            &alice(),
            Method::GET,
            "/.well-known/hlin.json",
            None,
            None,
            Token::Absent,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body["platform"]["id"], PLATFORM);
}

// -- The rules, one person at a time ------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Action {
    Read,
    Add,
    Toggle(Whose),
    Edit(Whose),
    Delete(Whose),
}

/// Which item an action is aimed at.
#[derive(Debug, Clone, Copy)]
enum Whose {
    /// Added to `team` by Alice, its owner.
    Alices,
    /// Added to `team` by Bob, a member.
    Bobs,
    /// Seeded onto `carol` by Carol.
    Carols,
}

#[tokio::test]
async fn members_read_add_and_toggle_and_only_authors_or_owners_edit_or_delete() {
    use Action::*;
    use Whose::*;

    let alice = alice();
    let bob = bob();
    let carol = carol();

    #[rustfmt::skip]
    let cases: Vec<(&Principal, Action, &str, StatusCode)> = vec![
        // Alice owns `team`: everything, including Bob's items.
        (&alice, Read,           "team",  StatusCode::OK),
        (&alice, Add,            "team",  StatusCode::CREATED),
        (&alice, Toggle(Bobs),   "team",  StatusCode::OK),
        (&alice, Edit(Alices),   "team",  StatusCode::OK),
        (&alice, Edit(Bobs),     "team",  StatusCode::OK),
        (&alice, Delete(Bobs),   "team",  StatusCode::NO_CONTENT),
        // Bob is a member: his own items, and ticking anybody's.
        (&bob,   Read,           "team",  StatusCode::OK),
        (&bob,   Add,            "team",  StatusCode::CREATED),
        (&bob,   Toggle(Alices), "team",  StatusCode::OK),
        (&bob,   Edit(Bobs),     "team",  StatusCode::OK),
        (&bob,   Delete(Bobs),   "team",  StatusCode::NO_CONTENT),
        (&bob,   Edit(Alices),   "team",  StatusCode::FORBIDDEN),
        (&bob,   Delete(Alices), "team",  StatusCode::FORBIDDEN),
        // Carol is not on `team` at all.
        (&carol, Read,           "team",  StatusCode::FORBIDDEN),
        (&carol, Add,            "team",  StatusCode::FORBIDDEN),
        (&carol, Toggle(Alices), "team",  StatusCode::FORBIDDEN),
        (&carol, Edit(Alices),   "team",  StatusCode::FORBIDDEN),
        (&carol, Delete(Bobs),   "team",  StatusCode::FORBIDDEN),
        // And `carol` is hers alone, owner included.
        (&carol, Read,           "carol", StatusCode::OK),
        (&carol, Add,            "carol", StatusCode::CREATED),
        (&carol, Edit(Carols),   "carol", StatusCode::OK),
        (&alice, Read,           "carol", StatusCode::FORBIDDEN),
        (&alice, Add,            "carol", StatusCode::FORBIDDEN),
        (&alice, Toggle(Carols), "carol", StatusCode::FORBIDDEN),
        (&alice, Delete(Carols), "carol", StatusCode::FORBIDDEN),
        // A list that does not exist is not there, for anybody.
        (&alice, Read,           "nope",  StatusCode::NOT_FOUND),
        (&alice, Add,            "nope",  StatusCode::NOT_FOUND),
    ];

    for (who, action, list, expected) in cases {
        // A fresh platform per case, so no case depends on another's writes.
        let platform = Platform::new();
        let alices = id_of(&platform.add(&alice, "team", "Alice's item").await);
        let bobs = id_of(&platform.add(&bob, "team", "Bob's item").await);
        let carols = "i4".to_string(); // the first item seeded onto `carol`
        let target = |whose: Whose| match whose {
            Alices => alices.clone(),
            Bobs => bobs.clone(),
            Carols => carols.clone(),
        };

        let items = format!("/api/lists/{list}/items");
        let answer = match action {
            Read => platform.get(who, &items).await,
            Add => {
                platform
                    .write(
                        who,
                        Method::POST,
                        &items,
                        Some(json!({ "text": "new" })),
                        None,
                    )
                    .await
            }
            Toggle(whose) => {
                let path = format!("{items}/{}/toggle", target(whose));
                platform.write(who, Method::POST, &path, None, None).await
            }
            Edit(whose) => {
                let path = format!("{items}/{}", target(whose));
                let body = Some(json!({ "text": "edited" }));
                platform.write(who, Method::PATCH, &path, body, None).await
            }
            Delete(whose) => {
                let path = format!("{items}/{}", target(whose));
                platform.write(who, Method::DELETE, &path, None, None).await
            }
        };

        let label = format!("{} {action:?} on {list}", who.sub);
        assert_eq!(answer.status, expected, "{label}: {}", answer.body);
        if !expected.is_success() {
            let message = answer.body["message"].as_str().unwrap_or_default();
            assert!(
                !message.is_empty(),
                "{label}: a refusal says why, in the platform's words"
            );
        }
    }
}

#[tokio::test]
async fn a_refusal_changes_nothing_and_says_whose_item_it_is() {
    let platform = Platform::new();
    let alices = id_of(&platform.add(&alice(), "team", "Alice's item").await);

    let answer = platform
        .write(
            &bob(),
            Method::PATCH,
            &format!("/api/lists/team/items/{alices}"),
            Some(json!({ "text": "Bob was here" })),
            None,
        )
        .await;

    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(
        answer.body,
        json!({ "message": "Only the person who added this item, or the list's owner, can edit or delete it." })
    );
    assert_eq!(
        platform.item("team", &alices).expect("still there").text,
        "Alice's item"
    );
}

#[tokio::test]
async fn an_item_records_who_added_it_by_name_and_by_id() {
    let platform = Platform::new();
    let answer = platform.add(&bob(), "team", "  Fix the printer ").await;

    assert_eq!(answer.status, StatusCode::CREATED);
    assert_eq!(answer.body["text"], "Fix the printer");
    assert_eq!(answer.body["done"], false);
    assert_eq!(answer.body["author"], "Bob");
    assert_eq!(answer.body["author_id"], "u-bob");
}

#[tokio::test]
async fn a_list_tells_its_own_module_who_the_viewer_is_to_it() {
    let platform = Platform::new();

    let as_alice = platform.get(&alice(), "/api/lists/team/items").await;
    assert_eq!(as_alice.status, StatusCode::OK);
    assert_eq!(
        as_alice.body["viewer"],
        json!({ "id": "u-alice", "owner": true })
    );
    assert_eq!(as_alice.body["list"]["owner"], "alice@example.com");
    assert_eq!(as_alice.body["items"].as_array().map(Vec::len), Some(3));

    let as_bob = platform.get(&bob(), "/api/lists/team/items").await;
    assert_eq!(as_bob.body["viewer"]["owner"], false);
}

#[tokio::test]
async fn each_person_is_offered_only_their_own_lists() {
    let platform = Platform::new();
    for (who, expected) in [
        (alice(), vec!["team"]),
        (bob(), vec!["team"]),
        (carol(), vec!["carol"]),
    ] {
        let answer = platform.get(&who, "/api/lists").await;
        let ids: Vec<&str> = answer.body["lists"]
            .as_array()
            .expect("lists")
            .iter()
            .filter_map(|list| list["id"].as_str())
            .collect();
        assert_eq!(ids, expected, "{}", who.sub);
    }
}

#[tokio::test]
async fn a_caller_with_no_email_is_on_no_list() {
    let platform = Platform::new();
    let nameless = Principal::new("u-anon");
    let answer = platform.get(&nameless, "/api/lists/team/items").await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert!(
        answer.body["message"]
            .as_str()
            .is_some_and(|m| m.contains("email"))
    );
}

// -- Write tokens must be bound to the write ----------------------------------

#[tokio::test]
async fn a_write_is_refused_unless_its_token_is_bound_to_that_request() {
    let platform = Platform::new();
    let path = "/api/lists/team/items/i1/toggle";

    for (token, why) in [
        (Token::Unbound, "a read token"),
        (
            Token::BoundTo("POST", "/api/lists/team/items"),
            "another path",
        ),
        (Token::BoundTo("DELETE", path), "another method"),
        (
            Token::BoundTo("POST", "/api/lists/carol/items/i4/toggle"),
            "another list",
        ),
        (Token::ForAnotherPlatform, "another platform"),
        #[cfg(not(feature = "dev-identity"))]
        (Token::Absent, "no token"),
    ] {
        let answer = platform
            .send(&alice(), Method::POST, path, None, None, token)
            .await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{why}");
        assert!(
            !platform.item("team", "i1").expect("seeded").done,
            "{why}: nothing changed"
        );
    }

    let answer = platform
        .send(
            &alice(),
            Method::POST,
            path,
            None,
            None,
            Token::AsTheShellWould,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "and bound, it goes through");
}

#[tokio::test]
async fn a_read_needs_no_binding_but_does_need_a_token() {
    let platform = Platform::new();
    let read = platform
        .send(
            &alice(),
            Method::GET,
            "/api/lists/team/items",
            None,
            None,
            Token::Unbound,
        )
        .await;
    assert_eq!(read.status, StatusCode::OK);

    let foreign = platform
        .send(
            &alice(),
            Method::GET,
            "/api/lists/team/items",
            None,
            None,
            Token::ForAnotherPlatform,
        )
        .await;
    assert_eq!(foreign.status, StatusCode::UNAUTHORIZED);
}

// -- A retried write is applied once ------------------------------------------

#[tokio::test]
async fn a_toggle_retried_with_its_key_is_applied_once() {
    let platform = Platform::new();
    let path = "/api/lists/team/items/i1/toggle";

    let first = platform
        .write(&bob(), Method::POST, path, None, Some("k-1"))
        .await;
    let again = platform
        .write(&bob(), Method::POST, path, None, Some("k-1"))
        .await;

    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(first.body, again.body, "the retry gets the first answer");
    assert_eq!(
        again.headers.get(REPLAYED).map(|v| v.as_bytes()),
        Some(&b"true"[..])
    );
    assert!(first.headers.get(REPLAYED).is_none());
    assert!(
        platform.item("team", "i1").expect("seeded").done,
        "toggled once, not twice"
    );

    // A new key is a new toggle.
    platform
        .write(&bob(), Method::POST, path, None, Some("k-2"))
        .await;
    assert!(!platform.item("team", "i1").expect("seeded").done);
}

#[tokio::test]
async fn an_add_retried_with_its_key_adds_one_item() {
    let platform = Platform::new();
    let body = Some(json!({ "text": "Only once" }));

    let first = platform
        .write(
            &alice(),
            Method::POST,
            "/api/lists/team/items",
            body.clone(),
            Some("k"),
        )
        .await;
    let again = platform
        .write(
            &alice(),
            Method::POST,
            "/api/lists/team/items",
            body,
            Some("k"),
        )
        .await;

    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(id_of(&first), id_of(&again));
    let listed = platform.get(&alice(), "/api/lists/team/items").await;
    let copies = listed.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|item| item["text"] == "Only once")
        .count();
    assert_eq!(copies, 1);
}

#[tokio::test]
async fn a_key_reused_for_a_different_change_is_refused() {
    let platform = Platform::new();
    platform
        .write(
            &alice(),
            Method::POST,
            "/api/lists/team/items",
            Some(json!({ "text": "one" })),
            Some("k"),
        )
        .await;
    let reused = platform
        .write(
            &alice(),
            Method::POST,
            "/api/lists/team/items",
            Some(json!({ "text": "two" })),
            Some("k"),
        )
        .await;
    assert_eq!(reused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reused.body["message"].is_string());
}

#[tokio::test]
async fn the_same_key_from_two_people_is_two_changes() {
    let platform = Platform::new();
    let path = "/api/lists/team/items/i1/toggle";
    platform
        .write(&alice(), Method::POST, path, None, Some("shared"))
        .await;
    let bobs = platform
        .write(&bob(), Method::POST, path, None, Some("shared"))
        .await;
    assert!(bobs.headers.get(REPLAYED).is_none());
    assert!(!platform.item("team", "i1").expect("seeded").done);
}

#[tokio::test]
async fn a_refused_write_retried_is_refused_again_in_the_same_words() {
    let platform = Platform::new();
    let path = "/api/lists/team/items/i1";
    let body = Some(json!({ "text": "mine now" }));

    let first = platform
        .write(&bob(), Method::PATCH, path, body.clone(), Some("k"))
        .await;
    let again = platform
        .write(&bob(), Method::PATCH, path, body, Some("k"))
        .await;
    assert_eq!(first.status, StatusCode::FORBIDDEN);
    assert_eq!(again.status, StatusCode::FORBIDDEN);
    assert_eq!(first.body, again.body);
}

// -- Every change is announced -------------------------------------------------

#[tokio::test]
async fn a_change_is_announced_with_its_list_after_it_lands() {
    let platform = Platform::new();
    let mut listening = platform.app.changes().subscribe();

    let added = platform.add(&alice(), "team", "Announce me").await;

    assert_eq!(listening.try_recv().expect("told"), "team");
    // Announced after the write, so a shell refetching on notice sees it.
    let id = id_of(&added);
    assert!(platform.item("team", &id).is_some());
}

#[tokio::test]
async fn a_refused_or_replayed_write_announces_nothing() {
    let platform = Platform::new();
    let path = "/api/lists/team/items/i1/toggle";
    platform
        .write(&bob(), Method::POST, path, None, Some("k"))
        .await;

    let mut listening = platform.app.changes().subscribe();
    platform
        .write(&bob(), Method::POST, path, None, Some("k"))
        .await;
    platform.add(&carol(), "team", "let me in").await;

    assert!(listening.try_recv().is_err(), "nothing changed");
}

#[test]
fn an_event_names_the_panel_and_narrows_to_the_list() {
    assert_eq!(
        changes::event_data(Some("team")),
        json!({ "panel": "items", "selections": { "list": ["team"] } })
    );
    assert_eq!(changes::event_data(None), json!({ "panel": "items" }));
}

#[tokio::test]
async fn the_event_stream_carries_a_change_to_a_shell_that_is_listening() {
    let platform = Platform::new();
    let token = platform.issuer.mint(&alice(), PLATFORM).expect("mints");
    let request = Request::get("/api/events")
        .header(IDENTITY_HEADER, token)
        .body(Body::empty())
        .expect("a request");
    let response = platform
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/event-stream",
        "server-sent events"
    );

    // The response has subscribed by now, so this change is queued for it.
    platform.add(&bob(), "team", "Heard about").await;

    let mut body = response.into_body();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame())
        .await
        .expect("an event within the timeout")
        .expect("a frame")
        .expect("not an error");
    let text = String::from_utf8(frame.into_data().expect("data").to_vec()).expect("utf-8");
    assert!(text.contains("event: changed"), "{text}");
    assert!(text.contains(r#""selections":{"list":["team"]}"#), "{text}");
}

#[cfg(not(feature = "dev-identity"))]
#[tokio::test]
async fn the_event_stream_is_behind_identity() {
    let platform = Platform::new();
    let answer = platform
        .send(
            &alice(),
            Method::GET,
            "/api/events",
            None,
            None,
            Token::Absent,
        )
        .await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
}

// -- The shell-drawn panel ------------------------------------------------------

#[tokio::test]
async fn the_panel_data_is_the_envelope_it_promised() {
    let platform = Platform::new();
    let answer = platform.get(&bob(), "/api/hlin/items?list=team").await;
    assert_eq!(answer.status, StatusCode::OK);

    let bytes = serde_json::to_vec(&answer.body).expect("serialises");
    let Envelope::Records(records) = parse_envelope(&bytes, "records.v1").expect("records.v1")
    else {
        panic!("records");
    };
    assert_eq!(records.rows.len(), 3);
    assert_eq!(records.rows[1]["done"], true);
}

#[tokio::test]
async fn the_panel_with_nothing_chosen_shows_the_viewers_first_list() {
    let platform = Platform::new();
    let answer = platform.get(&carol(), "/api/hlin/items").await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body["rows"][0]["text"], "Renew the domain");
}

#[tokio::test]
async fn the_panel_refuses_a_list_the_viewer_is_not_on() {
    let platform = Platform::new();
    let answer = platform.get(&carol(), "/api/hlin/items?list=team").await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_list_picker_offers_the_viewers_lists_as_options() {
    let platform = Platform::new();
    let answer = platform.get(&alice(), "/api/hlin/lists").await;
    let bytes = serde_json::to_vec(&answer.body).expect("serialises");
    let Envelope::Options(options) = parse_envelope(&bytes, "options.v1").expect("options.v1")
    else {
        panic!("options");
    };
    let values: Vec<&str> = options.options.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(values, vec!["team"]);
}
