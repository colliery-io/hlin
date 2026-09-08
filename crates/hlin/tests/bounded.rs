//! Reading a platform's answer without trusting how long it is.
//!
//! The 1 MiB envelope limit was always enforced; it was enforced on a buffer
//! that had already been allocated to whatever the platform sent. These drive a
//! real socket, because the behaviour under test is about how a body arrives —
//! declared length, chunked encoding, a stream that never ends — and none of
//! that survives being faked.

use std::convert::Infallible;

use axum::Router;
use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::get;
use hlin::bounded::{TooMuch, read_bounded};

const LIMIT: usize = 1024;

/// Serve these routes on a loopback port and give back its base URL.
async fn serving(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("a port");
    let port = listener.local_addr().expect("an address").port();

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn a_body_within_the_limit_is_read_whole() {
    let app = Router::new().route("/small", get(|| async { "x".repeat(LIMIT) }));
    let base = serving(app).await;

    let response = reqwest::get(format!("{base}/small"))
        .await
        .expect("answers");
    let body = read_bounded(response, LIMIT)
        .await
        .expect("within the limit");

    assert_eq!(body.len(), LIMIT, "a body exactly at the limit is allowed");
}

#[tokio::test]
async fn one_byte_over_the_limit_is_refused() {
    let app = Router::new().route("/big", get(|| async { "x".repeat(LIMIT + 1) }));
    let base = serving(app).await;

    let response = reqwest::get(format!("{base}/big")).await.expect("answers");

    match read_bounded(response, LIMIT).await {
        Err(TooMuch::Oversized { bytes, limit }) => {
            assert_eq!(limit, LIMIT);
            assert!(bytes > LIMIT, "reports at least what it saw");
        }
        other => panic!("expected an oversized refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_body_that_does_not_say_its_length_is_still_bounded() {
    // The important case, and the one `Content-Length` alone does not cover: a
    // chunked response declares no length, so the only way to refuse it is to
    // count while reading. A platform does not have to be malicious to send
    // one — it is what any streaming handler produces.
    let app = Router::new().route(
        "/chunked",
        get(|| async {
            let chunks = (0..64)
                .map(|_| Ok::<_, Infallible>("x".repeat(64)))
                .collect::<Vec<_>>();
            Body::from_stream(futures::stream::iter(chunks)).into_response()
        }),
    );
    let base = serving(app).await;

    let response = reqwest::get(format!("{base}/chunked"))
        .await
        .expect("answers");
    assert!(
        response.content_length().is_none(),
        "this test is only meaningful if the length is undeclared"
    );

    // 64 × 64 = 4096, four times the limit.
    match read_bounded(response, LIMIT).await {
        Err(TooMuch::Oversized { bytes, limit }) => {
            assert_eq!(limit, LIMIT);
            assert!(
                bytes <= LIMIT + 64,
                "gives up within one chunk of the limit rather than reading it all, saw {bytes}"
            );
        }
        other => panic!("expected an oversized refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_declared_length_is_refused_before_the_body_is_read() {
    // The cheap guard. A platform that says how long its answer is and means it
    // never costs the shell a read at all.
    let app = Router::new().route("/declared", get(|| async { "x".repeat(LIMIT * 8) }));
    let base = serving(app).await;

    let response = reqwest::get(format!("{base}/declared"))
        .await
        .expect("answers");
    assert!(
        response.content_length().is_some(),
        "this test is only meaningful if the length is declared"
    );

    match read_bounded(response, LIMIT).await {
        Err(TooMuch::Oversized { bytes, limit }) => {
            assert_eq!(limit, LIMIT);
            assert_eq!(
                bytes,
                LIMIT * 8,
                "the declared length is reported, so an operator sees what was claimed"
            );
        }
        other => panic!("expected an oversized refusal, got {other:?}"),
    }
}
