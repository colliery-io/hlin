//! The feed, checked the way the shell will call it.
//!
//! Every request here carries a token minted by a real `Issuer`, one per
//! person, as the shell's request proxy mints them: an ordinary token on a
//! read, and on a write a token bound to its method and its path relative to
//! the platform's base. The feed is served on a real socket so the identity
//! check sees the method and path the request actually arrived with.
//!
//! The people are the demo's: Alice and Bob at example.com, Carol elsewhere,
//! and Mo, at example.com but muted by this feed's own configuration.

use hlin_identity::{BoundRequest, Issuer, Principal, Verifier};
use hlin_manifest::{parse_envelope, validate};
use hlin_sample_feed::module::{self, ModuleFiles};
use hlin_sample_feed::posts::{Author, Post};
use hlin_sample_feed::rules::Rules;
use hlin_sample_feed::{Config, manifest, router};
use reqwest::StatusCode;
use serde_json::{Value, json};

const PLATFORM: &str = "feed";

// -- People ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Alice,
    Bob,
    Carol,
    Mo,
}

impl Who {
    fn principal(self) -> Principal {
        let (sub, name, email) = match self {
            Who::Alice => ("u-alice", "Alice", "alice@example.com"),
            Who::Bob => ("u-bob", "Bob", "bob@example.com"),
            Who::Carol => ("u-carol", "Carol", "carol@elsewhere.org"),
            Who::Mo => ("u-mo", "Mo", "mo@example.com"),
        };
        let mut principal = Principal::new(sub).with_name(name);
        principal.email = Some(email.to_string());
        principal
    }
}

/// One post by each of Alice, Bob and Mo, oldest first.
fn seed() -> Vec<Post> {
    let now = chrono::Utc::now();
    [(Who::Alice, "a"), (Who::Bob, "b"), (Who::Mo, "m")]
        .into_iter()
        .enumerate()
        .map(|(n, (who, id))| {
            let principal = who.principal();
            Post {
                id: id.to_string(),
                author: Author {
                    id: principal.sub,
                    name: principal.name.unwrap(),
                },
                body: format!("{who:?} was here"),
                posted_at: now - chrono::Duration::minutes(10 - n as i64),
                edited_at: None,
            }
        })
        .collect()
}

// -- A feed, and a shell's worth of calling it ------------------------------

struct Feed {
    base: String,
    issuer: Issuer,
    client: reqwest::Client,
}

async fn feed() -> Feed {
    let issuer = Issuer::generate("hlin");
    let config = Config {
        name: PLATFORM.to_string(),
        verifier: std::sync::Arc::new(Verifier::with_keys("hlin", issuer.jwks())),
        rules: Rules::new("example.com", ["mo@example.com"]),
        posts: seed(),
        module: ModuleFiles::none(),
    };

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router(config)).await.unwrap();
    });

    Feed {
        base,
        issuer,
        client: reqwest::Client::new(),
    }
}

/// What came back: the status, and the body as JSON where there was one.
struct Answer {
    status: StatusCode,
    body: Value,
    replayed: bool,
}

impl Feed {
    fn read_token(&self, who: Who) -> String {
        self.issuer.mint(&who.principal(), PLATFORM).unwrap()
    }

    fn write_token(&self, who: Who, method: &str, path: &str) -> String {
        let request = BoundRequest::new(method, path).unwrap();
        self.issuer
            .mint_bound(&who.principal(), PLATFORM, &request)
            .unwrap()
    }

    /// Send a request as the shell would: bound on a write, with a key.
    async fn call(&self, who: Who, method: &str, path: &str, body: Option<Value>) -> Answer {
        let token = if hlin_identity::is_read(method) {
            self.read_token(who)
        } else {
            self.write_token(who, method, path)
        };
        let key = unique_key();
        self.send(method, path, Some(&token), Some(&key), body)
            .await
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        key: Option<&str>,
        body: Option<Value>,
    ) -> Answer {
        let mut request = self
            .client
            .request(method.parse().unwrap(), format!("{}{}", self.base, path));
        if let Some(token) = token {
            request = request.header(hlin_identity::IDENTITY_HEADER, token);
        }
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let replayed = response.headers().contains_key("idempotency-replayed");
        let text = response.text().await.unwrap();
        let body = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Answer {
            status,
            body,
            replayed,
        }
    }

    async fn posts(&self) -> Vec<Value> {
        let answer = self.call(Who::Alice, "GET", "/api/posts", None).await;
        assert_eq!(answer.status, StatusCode::OK);
        answer.body["posts"].as_array().unwrap().clone()
    }

    async fn post(&self, id: &str) -> Option<Value> {
        self.posts().await.into_iter().find(|post| post["id"] == id)
    }
}

fn unique_key() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("key-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

// -- The manifest ---------------------------------------------------------

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    let document = manifest::build(PLATFORM);
    let checked = validate(&document, PLATFORM);

    assert!(checked.is_document_valid(), "{:?}", checked.document);
    assert!(checked.rejected().is_empty(), "{:?}", checked.rejected());
    assert!(
        checked.unusable_routes.is_empty(),
        "{:?}",
        checked.unusable_routes
    );
    assert_eq!(checked.unusable_events, None);
    assert_eq!(checked.accepted_keys(), vec![manifest::POSTS_PANEL]);
}

#[test]
fn the_manifest_round_trips_through_the_contract_crate() {
    let document = manifest::build(PLATFORM);
    let json = serde_json::to_string(&document).unwrap();
    assert_eq!(hlin_manifest::parse_str(&json).unwrap(), document);
}

#[test]
fn modules_may_read_and_write_under_api_and_nothing_else() {
    let routes = manifest::build(PLATFORM)
        .routes
        .expect("routes are declared");
    assert_eq!(routes.read, vec!["/api/"]);
    assert_eq!(routes.write, vec!["/api/"]);
}

#[test]
fn the_posts_panel_is_a_pushed_table_the_shell_can_draw_without_a_module() {
    let document = manifest::build(PLATFORM);
    let panel = document.panel(manifest::POSTS_PANEL).expect("declared");

    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert!(
        panel.pushed,
        "the feed announces every change, so it says so"
    );
    assert!(document.events.is_some());
}

#[test]
fn the_posts_panel_is_drawn_by_the_feeds_module_with_the_table_as_its_fallback() {
    let document = manifest::build(PLATFORM);
    let panel = document.panel(manifest::POSTS_PANEL).expect("declared");

    let ui = panel.ui.as_ref().expect("the module is declared");
    assert_eq!(ui.entry, module::POSTS_ENTRY);
    assert_eq!(ui.bridge, 1);
    assert_eq!(document.assets.as_deref(), Some(module::ASSETS));
    assert!(
        hlin_manifest::path::falls_under(module::ASSETS, &ui.entry),
        "the shell serves only what is under the assets prefix"
    );
    assert!(panel.data.is_some(), "the table fallback stays");
}

#[tokio::test]
async fn the_modules_files_are_served_to_anyone_and_nothing_else_under_the_prefix_is() {
    // The shell fetches module assets as itself, with no viewer identity:
    // code is the same for everybody. So no token here.
    let issuer = Issuer::generate("hlin");
    let config = Config {
        name: PLATFORM.to_string(),
        verifier: std::sync::Arc::new(Verifier::with_keys("hlin", issuer.jwks())),
        rules: Rules::new("example.com", Vec::<String>::new()),
        posts: seed(),
        module: ModuleFiles::from_files([
            ("index.html".to_string(), b"<!doctype html>".to_vec()),
            ("boot.js".to_string(), b"// boot".to_vec()),
        ]),
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router(config)).await.unwrap();
    });
    let client = reqwest::Client::new();
    let get = |path: &str| client.get(format!("{base}{path}")).send();

    let entry = get(module::POSTS_ENTRY).await.unwrap();
    assert_eq!(entry.status(), StatusCode::OK);
    assert!(
        entry.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(
        get("/ui/posts/boot.js").await.unwrap().status(),
        StatusCode::OK
    );
    for missing in [
        "/ui/posts/missing.js",
        "/ui/posts/%2E%2E%2Fboot.js",
        "/ui/boot.js",
    ] {
        assert_eq!(
            get(missing).await.unwrap().status(),
            StatusCode::NOT_FOUND,
            "{missing}"
        );
    }
}

// -- The rules, one person at a time --------------------------------------

#[derive(Debug, Clone, Copy)]
enum Action {
    Read,
    Post,
    /// Edit the seeded post with this id.
    Edit(&'static str),
    /// Delete the seeded post with this id.
    Delete(&'static str),
}

const ALICES: &str = "a";
const MOS: &str = "m";

#[tokio::test]
async fn who_may_do_what_is_decided_from_the_token_and_the_feeds_own_data() {
    use Action::*;
    use Who::*;

    // (who, does what, expected status, the feed's words where it refuses)
    let cases: &[(Who, Action, u16, Option<&str>)] = &[
        // Anyone signed in reads.
        (Alice, Read, 200, None),
        (Bob, Read, 200, None),
        (Carol, Read, 200, None),
        (Mo, Read, 200, None),
        // Only example.com posts, and not the muted.
        (Alice, Post, 201, None),
        (Bob, Post, 201, None),
        (
            Carol,
            Post,
            403,
            Some("Only people at example.com can post here"),
        ),
        (Mo, Post, 403, Some("You have been muted on this feed")),
        // Authors edit their own; nobody else's.
        (Alice, Edit(ALICES), 200, None),
        (
            Bob,
            Edit(ALICES),
            403,
            Some("Only the author can edit this post"),
        ),
        (
            Carol,
            Edit(ALICES),
            403,
            Some("Only the author can edit this post"),
        ),
        (
            Mo,
            Edit(ALICES),
            403,
            Some("Only the author can edit this post"),
        ),
        // Rewriting is posting, so the muted may not edit even their own.
        (Mo, Edit(MOS), 403, Some("You have been muted on this feed")),
        // Authors delete their own; nobody else's.
        (Alice, Delete(ALICES), 204, None),
        (
            Bob,
            Delete(ALICES),
            403,
            Some("Only the author can delete this post"),
        ),
        (
            Carol,
            Delete(ALICES),
            403,
            Some("Only the author can delete this post"),
        ),
        (
            Mo,
            Delete(ALICES),
            403,
            Some("Only the author can delete this post"),
        ),
        // Taking one's own words down is never refused.
        (Mo, Delete(MOS), 204, None),
        // A post that is not there is not there, whoever asks.
        (Alice, Edit("nope"), 404, Some("There is no such post")),
        (Carol, Delete("nope"), 404, Some("There is no such post")),
    ];

    for &(who, action, expected, words) in cases {
        // A fresh feed per case, so a delete in one cannot decide another.
        let feed = feed().await;
        let before = feed.posts().await;

        let answer = match action {
            Read => feed.call(who, "GET", "/api/posts", None).await,
            Post => {
                feed.call(who, "POST", "/api/posts", Some(json!({ "body": "hello" })))
                    .await
            }
            Edit(id) => {
                feed.call(
                    who,
                    "PUT",
                    &format!("/api/posts/{id}"),
                    Some(json!({ "body": "changed" })),
                )
                .await
            }
            Delete(id) => {
                feed.call(who, "DELETE", &format!("/api/posts/{id}"), None)
                    .await
            }
        };

        let case = format!("{who:?} {action:?}");
        assert_eq!(answer.status.as_u16(), expected, "{case}: {}", answer.body);

        if let Some(words) = words {
            assert_eq!(
                answer.body,
                json!({ "message": words }),
                "{case}: a refusal is the feed's own words and nothing else"
            );
        }

        // A refusal changed nothing; a success changed what it said it did.
        let after = feed.posts().await;
        if expected >= 400 {
            assert_eq!(before, after, "{case}: a refused write must change nothing");
        }
        match (action, expected) {
            (Post, 201) => {
                assert_eq!(after.len(), before.len() + 1, "{case}");
                assert_eq!(after[0]["body"], "hello", "{case}: newest first");
                assert_eq!(after[0]["author"]["id"], who.principal().sub, "{case}");
            }
            (Edit(id), 200) => {
                let edited = feed.post(id).await.unwrap();
                assert_eq!(edited["body"], "changed", "{case}");
                assert!(edited["edited_at"].is_string(), "{case}");
            }
            (Delete(id), 204) => assert!(feed.post(id).await.is_none(), "{case}"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn a_post_needs_words_and_not_too_many() {
    let feed = feed().await;

    for body in [json!({ "body": "   " }), json!({ "body": "x".repeat(501) })] {
        let answer = feed
            .call(Who::Alice, "POST", "/api/posts", Some(body))
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert!(answer.body["message"].is_string());
    }

    let answer = feed
        .call(
            Who::Alice,
            "POST",
            "/api/posts",
            Some(json!({ "text": "hi" })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
}

// -- Credentials ----------------------------------------------------------

// Not with the development bypass, whose whole purpose is to answer an
// unsigned request as somebody.
#[cfg(not(feature = "dev-identity"))]
#[tokio::test]
async fn nobody_reads_or_writes_without_a_token() {
    let feed = feed().await;

    let read = feed.send("GET", "/api/posts", None, None, None).await;
    assert_eq!(read.status, StatusCode::UNAUTHORIZED);

    let write = feed
        .send(
            "POST",
            "/api/posts",
            None,
            Some("k"),
            Some(json!({ "body": "hi" })),
        )
        .await;
    assert_eq!(write.status, StatusCode::UNAUTHORIZED);
    assert_eq!(feed.posts().await.len(), 3);
}

#[tokio::test]
async fn a_read_token_cannot_be_spent_on_a_write() {
    let feed = feed().await;
    let unbound = feed.read_token(Who::Alice);

    let answer = feed
        .send(
            "POST",
            "/api/posts",
            Some(&unbound),
            Some("k"),
            Some(json!({ "body": "hi" })),
        )
        .await;

    assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{}", answer.body);
    assert!(
        answer.body["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "a platform team debugging this needs to be told why: {}",
        answer.body
    );
    assert_eq!(feed.posts().await.len(), 3);
}

#[tokio::test]
async fn a_write_token_is_good_for_its_own_method_and_path_only() {
    let feed = feed().await;

    // Minted for deleting Alice's post; spent on deleting another post, on
    // editing the same one, and on another route altogether. Look-alike paths
    // are `hlin-identity`'s to test; what matters here is that the feed asks.
    let for_a = feed.write_token(Who::Alice, "DELETE", "/api/posts/a");
    let attempts = [
        ("DELETE", "/api/posts/b"),
        ("PUT", "/api/posts/a"),
        ("POST", "/api/posts"),
    ];
    for (method, path) in attempts {
        let answer = feed
            .send(method, path, Some(&for_a), Some(&unique_key()), None)
            .await;
        assert_eq!(
            answer.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path}: {}",
            answer.body
        );
    }
    assert_eq!(feed.posts().await.len(), 3, "nothing was deleted");

    // And for the request it was minted for, it works.
    let answer = feed
        .send(
            "DELETE",
            "/api/posts/a",
            Some(&for_a),
            Some(&unique_key()),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_token_for_another_platform_or_from_another_shell_is_refused() {
    let feed = feed().await;

    let elsewhere = feed
        .issuer
        .mint(&Who::Alice.principal(), "checklist")
        .unwrap();
    let impostor = Issuer::generate("hlin")
        .mint(&Who::Alice.principal(), PLATFORM)
        .unwrap();

    for token in [elsewhere, impostor] {
        let answer = feed
            .send("GET", "/api/posts", Some(&token), None, None)
            .await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
}

// -- Idempotency ----------------------------------------------------------

#[tokio::test]
async fn the_same_write_sent_twice_with_one_key_is_made_once() {
    let feed = feed().await;
    let key = unique_key();
    let body = json!({ "body": "only once" });

    // A fresh bound token each time, as the shell mints one per request.
    let mut answers = Vec::new();
    for _ in 0..2 {
        let token = feed.write_token(Who::Alice, "POST", "/api/posts");
        answers.push(
            feed.send(
                "POST",
                "/api/posts",
                Some(&token),
                Some(&key),
                Some(body.clone()),
            )
            .await,
        );
    }

    assert_eq!(answers[0].status, StatusCode::CREATED);
    assert_eq!(answers[1].status, StatusCode::CREATED);
    assert_eq!(answers[0].body, answers[1].body, "the same answer, again");
    assert!(!answers[0].replayed);
    assert!(answers[1].replayed);

    let posts = feed.posts().await;
    assert_eq!(
        posts
            .iter()
            .filter(|post| post["body"] == "only once")
            .count(),
        1
    );
}

#[tokio::test]
async fn a_key_reused_for_a_different_write_is_refused() {
    let feed = feed().await;
    let key = unique_key();

    for (body, expected) in [
        ("first", StatusCode::CREATED),
        ("second", StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        let token = feed.write_token(Who::Alice, "POST", "/api/posts");
        let answer = feed
            .send(
                "POST",
                "/api/posts",
                Some(&token),
                Some(&key),
                Some(json!({ "body": body })),
            )
            .await;
        assert_eq!(answer.status, expected, "{}", answer.body);
    }
    assert_eq!(feed.posts().await.len(), 4);
}

#[tokio::test]
async fn one_persons_key_says_nothing_about_anothers_write() {
    let feed = feed().await;
    let key = "shared-by-accident";

    for who in [Who::Alice, Who::Bob] {
        let token = feed.write_token(who, "POST", "/api/posts");
        let answer = feed
            .send(
                "POST",
                "/api/posts",
                Some(&token),
                Some(key),
                Some(json!({ "body": "same words" })),
            )
            .await;
        assert_eq!(answer.status, StatusCode::CREATED);
        assert!(
            !answer.replayed,
            "{who:?} must not be shown another's answer"
        );
        assert_eq!(answer.body["author"]["id"], who.principal().sub);
    }
    assert_eq!(feed.posts().await.len(), 5);
}

#[tokio::test]
async fn a_write_without_a_key_is_refused() {
    let feed = feed().await;
    let token = feed.write_token(Who::Alice, "POST", "/api/posts");

    let answer = feed
        .send(
            "POST",
            "/api/posts",
            Some(&token),
            None,
            Some(json!({ "body": "hi" })),
        )
        .await;

    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        answer.body,
        json!({ "message": "Writes need an Idempotency-Key" })
    );
    assert_eq!(feed.posts().await.len(), 3);
}

#[tokio::test]
async fn a_refused_write_is_decided_again_when_retried() {
    // Nothing happened the first time, so nothing is remembered: a retry with
    // the same key after fixing the problem is a new decision.
    let feed = feed().await;
    let key = unique_key();

    let too_long = json!({ "body": "x".repeat(501) });
    let token = feed.write_token(Who::Alice, "POST", "/api/posts");
    let first = feed
        .send(
            "POST",
            "/api/posts",
            Some(&token),
            Some(&key),
            Some(too_long),
        )
        .await;
    assert_eq!(first.status, StatusCode::BAD_REQUEST);

    let token = feed.write_token(Who::Alice, "POST", "/api/posts");
    let second = feed
        .send(
            "POST",
            "/api/posts",
            Some(&token),
            Some(&key),
            Some(json!({ "body": "shorter" })),
        )
        .await;
    assert_eq!(second.status, StatusCode::CREATED);
}

// -- The fallback panel ---------------------------------------------------

#[tokio::test]
async fn the_panel_endpoint_returns_the_envelope_it_declared_newest_first() {
    let feed = feed().await;
    feed.call(
        Who::Bob,
        "POST",
        "/api/posts",
        Some(json!({ "body": "latest" })),
    )
    .await;

    let answer = feed
        .call(
            Who::Carol,
            "GET",
            &format!("/{}", manifest::POSTS_DATA),
            None,
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    let bytes = serde_json::to_vec(&answer.body).unwrap();
    parse_envelope(&bytes, "records.v1").expect("the declared envelope");

    let rows = answer.body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["post"], "latest");
    assert_eq!(rows[0]["author"], "Bob");
}

// -- The event stream -----------------------------------------------------

#[tokio::test]
async fn every_change_is_announced_on_the_event_stream() {
    let feed = feed().await;

    let stream = feed
        .client
        .get(format!("{}/{}", feed.base, manifest::EVENTS))
        .header(hlin_identity::IDENTITY_HEADER, feed.read_token(Who::Carol))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), StatusCode::OK);

    let created = feed
        .call(
            Who::Alice,
            "POST",
            "/api/posts",
            Some(json!({ "body": "news" })),
        )
        .await;
    let id = created.body["id"].as_str().unwrap().to_string();
    feed.call(
        Who::Alice,
        "PUT",
        &format!("/api/posts/{id}"),
        Some(json!({ "body": "news, corrected" })),
    )
    .await;
    feed.call(Who::Alice, "DELETE", &format!("/api/posts/{id}"), None)
        .await;
    let seen = watch_for(stream, 3, std::time::Duration::from_secs(5))
        .await
        .expect("create, edit and delete were each announced");
    assert!(seen.contains(r#""panel":"posts""#), "{seen}");
    assert!(!seen.contains("news"), "news, not data: {seen}");
}

#[cfg(not(feature = "dev-identity"))]
#[tokio::test]
async fn the_event_stream_is_behind_identity_too() {
    let feed = feed().await;
    let answer = feed.send("GET", "/api/events", None, None, None).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
}

/// Read a held-open response until `count` `changed` events have arrived.
async fn watch_for(
    response: reqwest::Response,
    count: usize,
    patience: std::time::Duration,
) -> Option<String> {
    use futures::StreamExt;

    let mut body = response.bytes_stream();
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + patience;

    loop {
        match tokio::time::timeout_at(deadline, body.next()).await {
            Ok(Some(Ok(chunk))) => {
                seen.push_str(&String::from_utf8_lossy(&chunk));
                if seen.matches("event: changed").count() >= count {
                    return Some(seen);
                }
            }
            _ => return None,
        }
    }
}
