//! What every widget gets from this crate, checked once on a toy widget: a
//! list anyone may add a word to.
//!
//! The widgets' own tests check their rules. These check what none of them
//! should have to: that a write needs a token bound to it and a key, that a
//! key is honoured per person and bound to its request, that a refusal
//! changes nothing and is not remembered, that the manifest, the module's
//! files and the event stream are where the manifest says, and that the
//! widget's origin is laid out as a real platform's is: its own UI at the
//! root, Hlin's surface under `/hlin`, and nothing of one answered by the
//! other.

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hlin_widget_support::envelope::{ColumnType, Envelope, Line, Point, Records, Series};
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{
    Fallback, ModuleFiles, Platform, Refusal, Reply, Site, Viewer, Widget, Write,
};
use serde_json::json;

type Words = Vec<String>;

fn widget() -> Widget<Words> {
    Widget {
        panel: "words",
        name: "Words",
        icon: "list",
        title: "Words",
        description: "Words anyone may add",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |words, _| {
                Envelope::Records(Records {
                    columns: vec![hlin_widget_support::envelope::Column {
                        key: "word".to_string(),
                        label: "Word".to_string(),
                        value_type: ColumnType::String,
                        unit: None,
                        extra: Default::default(),
                    }],
                    rows: words
                        .iter()
                        .map(|word| {
                            let mut row = serde_json::Map::new();
                            row.insert("word".to_string(), json!(word));
                            row
                        })
                        .collect(),
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: "/nowhere",
    }
}

fn api() -> Router<Platform<Words>> {
    async fn list(State(platform): State<Platform<Words>>, Viewer(_): Viewer) -> Response {
        axum::Json(platform.read(Words::clone)).into_response()
    }
    async fn add(State(platform): State<Platform<Words>>, write: Write) -> Response {
        platform.write(&write, |words, _| {
            let word: String = write.json("A word is a JSON string")?;
            if word.contains(' ') {
                return Err(Refusal::bad_request("One word, no spaces"));
            }
            words.push(word);
            Ok(Reply::created(&*words))
        })
    }
    Router::new()
        .route("/api/words", get(list))
        .route("/api/words/add", post(add))
}

async fn words() -> Running {
    testing::start(widget(), Words::new(), api()).await
}

#[test]
fn the_toy_widget_is_a_manifest_the_shell_accepts() {
    assert_eq!(widget().defects("words"), None);
}

#[tokio::test]
async fn the_manifest_and_health_are_served_to_anyone() {
    let words = words().await;
    let manifest = words
        .send("GET", "/.well-known/hlin.json", None, None, None)
        .await;
    assert_eq!(manifest.status, 200);
    assert_eq!(manifest.body["platform"]["id"], "words");
    assert_eq!(
        manifest.body["panels"][0]["ui"]["entry"],
        "/ui/words/index.html"
    );
    assert_eq!(manifest.body["events"], "api/events");

    let health = words.send("GET", "/api/health", None, None, None).await;
    assert_eq!(health.status, 200);
}

#[tokio::test]
async fn reads_and_the_event_stream_need_a_token() {
    let words = words().await;
    for path in ["/api/words", "/api/events", "/api/panels/words"] {
        let answer = words.send("GET", path, None, None, None).await;
        assert_eq!(answer.status, 401, "{path}");
    }
}

#[tokio::test]
async fn a_write_needs_a_token_bound_to_it() {
    let words = words().await;
    let alice = person("u-alice", "Alice");

    // An ordinary read token, as might be lifted from a log.
    let read_token = words.read_token(&alice);
    let refused = words
        .send(
            "POST",
            "/api/words/add",
            Some(&read_token),
            Some("k1"),
            Some(json!("hi")),
        )
        .await;
    assert_eq!(refused.status, 401);

    // Bound, but to somewhere else.
    let elsewhere = words.write_token(&alice, "POST", "/api/other");
    let refused = words
        .send(
            "POST",
            "/api/words/add",
            Some(&elsewhere),
            Some("k1"),
            Some(json!("hi")),
        )
        .await;
    assert_eq!(refused.status, 401);

    assert_eq!(words.get(&alice, "/api/words").await.body, json!([]));
}

#[tokio::test]
async fn a_write_needs_an_idempotency_key_it_can_keep() {
    let words = words().await;
    let alice = person("u-alice", "Alice");
    let token = words.write_token(&alice, "POST", "/api/words/add");

    let none = words
        .send(
            "POST",
            "/api/words/add",
            Some(&token),
            None,
            Some(json!("hi")),
        )
        .await;
    assert_eq!(none.status, 400);
    assert_eq!(
        none.body,
        json!({ "message": "Writes need an Idempotency-Key" })
    );

    let spaced = words
        .send(
            "POST",
            "/api/words/add",
            Some(&token),
            Some("has space"),
            Some(json!("hi")),
        )
        .await;
    assert_eq!(spaced.status, 400);
}

#[tokio::test]
async fn a_key_is_per_person_and_bound_to_its_request() {
    let words = words().await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));
    let add = |word: &str| Some(json!(word));

    let first = words
        .write_keyed(&alice, "POST", "/api/words/add", add("one"), "k1")
        .await;
    assert_eq!(first.status, 201);

    // Alice again, same request: answered as before, not done twice.
    let again = words
        .write_keyed(&alice, "POST", "/api/words/add", add("one"), "k1")
        .await;
    assert!(again.replayed);
    assert_eq!(again.body, first.body);

    // Alice, same key, different request: a client bug, said so.
    let conflict = words
        .write_keyed(&alice, "POST", "/api/words/add", add("two"), "k1")
        .await;
    assert_eq!(conflict.status, 422);

    // Bob's k1 is his own.
    let bobs = words
        .write_keyed(&bob, "POST", "/api/words/add", add("two"), "k1")
        .await;
    assert_eq!(bobs.status, 201);
    assert!(!bobs.replayed);

    assert_eq!(
        words.get(&alice, "/api/words").await.body,
        json!(["one", "two"])
    );
}

#[tokio::test]
async fn a_refusal_changes_nothing_and_a_retry_is_decided_again() {
    let words = words().await;
    let alice = person("u-alice", "Alice");

    let refused = words
        .write_keyed(
            &alice,
            "POST",
            "/api/words/add",
            Some(json!("two words")),
            "k1",
        )
        .await;
    assert_eq!(refused.status, 400);
    assert_eq!(refused.body, json!({ "message": "One word, no spaces" }));

    let malformed = words
        .write(
            &alice,
            "POST",
            "/api/words/add",
            Some(json!({ "not": "a string" })),
        )
        .await;
    assert_eq!(
        malformed.body,
        json!({ "message": "A word is a JSON string" })
    );

    let again = words
        .write_keyed(
            &alice,
            "POST",
            "/api/words/add",
            Some(json!("two words")),
            "k1",
        )
        .await;
    assert!(!again.replayed, "a refusal is not remembered");
    assert_eq!(words.get(&alice, "/api/words").await.body, json!([]));
}

#[tokio::test]
async fn a_write_that_happened_is_announced_and_a_refusal_is_not() {
    let words = words().await;
    let alice = person("u-alice", "Alice");
    let mut events = words.listen(&alice).await;

    words
        .write(&alice, "POST", "/api/words/add", Some(json!("two words")))
        .await;
    words
        .write(&alice, "POST", "/api/words/add", Some(json!("one")))
        .await;

    // The first `changed` is the add; had the refusal been announced, a
    // second would follow at once.
    assert_eq!(events.changed().await, Some(json!({ "panel": "words" })));
    words
        .write(&alice, "POST", "/api/words/add", Some(json!("more")))
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": "words" })));
}

#[tokio::test]
async fn the_fallback_is_the_envelope_the_manifest_promised() {
    let words = words().await;
    let alice = person("u-alice", "Alice");
    words
        .write(&alice, "POST", "/api/words/add", Some(json!("one")))
        .await;

    let Envelope::Records(table) = words.fallback(&alice).await else {
        panic!("a records envelope");
    };
    assert_eq!(table.rows.len(), 1);
}

#[tokio::test]
async fn the_modules_files_are_served_to_anyone_and_nothing_else_is() {
    let files = ModuleFiles::from_files([
        ("index.html".to_string(), b"<!doctype html>".to_vec()),
        ("boot.js".to_string(), b"// boot".to_vec()),
    ]);
    let words = testing::start_with(widget(), Words::new(), api(), files).await;

    let entry = words
        .send("GET", "/ui/words/index.html", None, None, None)
        .await;
    assert_eq!(entry.status, 200);
    for missing in [
        "/ui/words/missing.js",
        "/ui/words/%2E%2E%2Fboot.js",
        "/ui/boot.js",
    ] {
        let answer = words.send("GET", missing, None, None, None).await;
        assert_eq!(answer.status, 404, "{missing}");
    }
}

/// A widget whose fallback is a series: one point a minute, from the epoch.
fn ticks() -> Widget<Vec<i64>> {
    Widget {
        panel: "ticks",
        name: "Ticks",
        icon: "chart",
        title: "Ticks",
        description: "A point a minute",
        shared: false,
        fallback: Some(Fallback {
            kind: "timeseries",
            envelope: "series.v1",
            refresh_ms: None,
            data: |minutes, _| {
                Envelope::Series(Series {
                    series: vec![Line {
                        name: "ticks".to_string(),
                        points: minutes
                            .iter()
                            .map(|minute| Point(minute * 60_000, Some(*minute as f64)))
                            .collect(),
                        extra: Default::default(),
                    }],
                    unit: None,
                    as_of: None,
                    extra: Default::default(),
                })
            },
        }),
        built: "/nowhere",
    }
}

#[tokio::test]
async fn a_series_fallback_follows_the_time_range_and_is_cut_to_it() {
    let document = ticks().manifest("ticks");
    let params: Vec<_> = document.panel("ticks").unwrap().params.clone();
    assert_eq!(
        params.iter().map(|p| p.param.as_str()).collect::<Vec<_>>(),
        ["time_range"],
        "a series is over the surface's time range"
    );
    assert_eq!(ticks().defects("ticks"), None);
    let words = widget().manifest("words");
    assert!(
        words.panel("words").unwrap().params.is_empty(),
        "a table is not"
    );

    let ticks = testing::start(ticks(), (0..10).collect(), Router::new()).await;
    let alice = person("u-alice", "Alice");
    let minutes = |envelope: Envelope| match envelope {
        Envelope::Series(series) => series.series[0]
            .points
            .iter()
            .map(|point| point.at() / 60_000)
            .collect::<Vec<_>>(),
        other => panic!("a series, not {other:?}"),
    };

    assert_eq!(
        minutes(ticks.fallback(&alice).await),
        (0..10).collect::<Vec<_>>()
    );
    // From inclusive, to exclusive; `step` is only a hint.
    let asked = ticks
        .fallback_asking(
            &alice,
            "from=1970-01-01T00:03:00Z&to=1970-01-01T00:06:00Z&step=60",
        )
        .await;
    assert_eq!(minutes(asked), [3, 4, 5]);
}

// -- The origin: the widget's own UI at the root, Hlin under /hlin ----------

/// The toy widget with a UI of its own and a module, `/api/` as `local_user`.
async fn laid_out(local_user: Option<&str>) -> Running {
    let module = ModuleFiles::from_files([
        ("index.html".to_string(), b"module entry".to_vec()),
        ("boot.js".to_string(), b"// boot".to_vec()),
    ]);
    let ui = ModuleFiles::from_files([
        ("index.html".to_string(), b"the widget's own page".to_vec()),
        ("ui-0123456789abcdef.js".to_string(), b"// ui".to_vec()),
    ]);
    let layout = Site {
        local_user: local_user.map(str::to_string),
        ui: Some(ui),
        ..Site::default()
    };
    testing::start_site(widget(), Words::new(), api(), module, layout).await
}

#[tokio::test]
async fn hlins_surface_is_under_its_base_and_the_widgets_own_ui_is_at_the_root() {
    let words = laid_out(None).await;
    assert!(words.base.ends_with("/hlin"));

    let manifest = words
        .own("GET", "/hlin/.well-known/hlin.json", None, None)
        .await;
    assert_eq!(manifest.body["platform"]["id"], "words");
    let entry = words
        .own("GET", "/hlin/ui/words/index.html", None, None)
        .await;
    assert_eq!(entry.body, json!("module entry"));

    for page in ["/", "/index.html", "/lists/today"] {
        let answer = words.own("GET", page, None, None).await;
        assert_eq!(answer.status, 200, "{page}");
        assert_eq!(answer.body, json!("the widget's own page"), "{page}");
    }
    let script = words
        .own("GET", "/ui-0123456789abcdef.js", None, None)
        .await;
    assert_eq!(script.body, json!("// ui"));
}

#[tokio::test]
async fn nothing_unknown_under_hlin_is_ever_answered_with_the_widgets_own_page() {
    let words = laid_out(Some("Dana")).await;
    for missing in [
        // A module file that is not in the build.
        "/hlin/ui/words/missing.js",
        "/hlin/ui/words/nope",
        // A route the widget does not have.
        "/hlin/api/nope",
        "/hlin/nope",
        "/hlin",
        // The manifest's path, anywhere but under the base.
        "/.well-known/hlin.json",
        "/hlin/hlin/.well-known/hlin.json",
        // The widget's own API, and a file its UI does not have.
        "/api/nope",
        "/nope.js",
    ] {
        let answer = words.own("GET", missing, None, None).await;
        assert_eq!(answer.status, 404, "{missing}");
        assert_ne!(answer.body, json!("the widget's own page"), "{missing}");
    }
}

#[tokio::test]
async fn the_widgets_own_api_is_refused_unless_a_local_user_is_named() {
    let words = laid_out(None).await;
    let refused = words.own("GET", "/api/words", None, None).await;
    assert_eq!(refused.status, 401);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .contains("--local-user")
    );
    let refused = words
        .own("POST", "/api/words/add", Some("k1"), Some(json!("hi")))
        .await;
    assert_eq!(refused.status, 401);
    // Health answers whoever asks, as under Hlin's base.
    assert_eq!(
        words.own("GET", "/api/health", None, None).await.status,
        200
    );
}

#[tokio::test]
async fn the_widgets_own_ui_and_hlin_share_the_same_handlers_and_data() {
    let words = laid_out(Some("Dana")).await;
    let alice = person("u-alice", "Alice");
    let mut hlin_hears = words.listen(&alice).await;

    // A write from the widget's own page: no token, a key, and announced on
    // the stream Hlin listens to.
    let added = words
        .own("POST", "/api/words/add", Some("k1"), Some(json!("one")))
        .await;
    assert_eq!(added.status, 201, "{}", added.body);
    assert_eq!(
        hlin_hears.changed().await,
        Some(json!({ "panel": "words" }))
    );
    assert_eq!(words.get(&alice, "/api/words").await.body, json!(["one"]));

    // And one from Hlin, seen by the widget's own page.
    words
        .write(&alice, "POST", "/api/words/add", Some(json!("two")))
        .await;
    let seen = words.own("GET", "/api/words", None, None).await;
    assert_eq!(seen.body, json!(["one", "two"]));

    // Its own page's writes need a key as Hlin's do.
    let keyless = words
        .own("POST", "/api/words/add", None, Some(json!("three")))
        .await;
    assert_eq!(keyless.status, 400);
}

#[tokio::test]
async fn a_local_user_never_reaches_hlins_routes() {
    let words = laid_out(Some("Dana")).await;
    for path in ["/hlin/api/words", "/hlin/api/events"] {
        let answer = words.own("GET", path, None, None).await;
        assert_eq!(answer.status, 401, "{path}");
    }
}
