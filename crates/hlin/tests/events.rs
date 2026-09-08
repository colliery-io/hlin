//! Following a platform's event stream, against real sockets.
//!
//! Every case here holds an actual TCP connection, because what is under test is
//! a response that never ends and the failures worth catching are things a mock
//! transport cannot do: closing mid-stream, answering with the wrong content
//! type, and — the one with no signal of its own — staying open and saying
//! nothing.
//!
//! The servers are hand-written rather than axum handlers on purpose. Several of
//! them are deliberately not well-behaved HTTP, which is the point: a platform
//! on a bad day is not a platform using a framework correctly.

use std::time::Duration;

use hlin::stream::events::{self, Changed, Ended};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// A server that writes whatever a script says and then does what it is told.
///
/// Returns the base URL. The connection is held by the spawned task, so a script
/// that never finishes is a stream that stays open.
async fn serving(script: Vec<Act>) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();

        // Read past the request line and headers. Nothing here cares what they
        // said, only that the client is waiting for an answer.
        let mut scratch = [0u8; 2048];
        let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut scratch).await;

        for act in script {
            match act {
                Act::Head(head) => {
                    socket.write_all(head.as_bytes()).await.unwrap();
                    socket.flush().await.unwrap();
                }
                Act::Say(text) => {
                    // Chunked, because that is what a real streaming response
                    // is and the chunk boundaries are where an event gets split
                    // in half.
                    let chunk = format!("{:x}\r\n{text}\r\n", text.len());
                    if socket.write_all(chunk.as_bytes()).await.is_err() {
                        return;
                    }
                    let _ = socket.flush().await;
                }
                Act::Wait(how_long) => tokio::time::sleep(how_long).await,
                Act::Close => {
                    // The terminating zero-length chunk. A platform shutting a
                    // stream down cleanly sends this; one whose process dies
                    // does not, and the two are different failures.
                    let _ = socket.write_all(b"0\r\n\r\n").await;
                    let _ = socket.flush().await;
                    return;
                }
                Act::Vanish => return,
                Act::HoldForever => {
                    // Never writes, never closes. The failure this whole
                    // heartbeat mechanism exists to notice.
                    std::future::pending::<()>().await;
                }
            }
        }
    });

    base
}

enum Act {
    Head(String),
    Say(String),
    Wait(Duration),
    /// Close the stream properly.
    Close,
    /// Stop existing mid-body, the way a process that died does.
    Vanish,
    HoldForever,
}

fn stream_head() -> Act {
    Act::Head(
        "HTTP/1.1 200 OK\r\n\
         content-type: text/event-stream\r\n\
         transfer-encoding: chunked\r\n\r\n"
            .to_string(),
    )
}

fn event(panel: &str) -> Act {
    Act::Say(format!(
        "event: changed\ndata: {{\"panel\":\"{panel}\"}}\n\n"
    ))
}

async fn follow(base: &str) -> (Ended, Vec<Changed>) {
    let (tell, mut heard) = mpsc::channel(16);
    let client = reqwest::Client::new();

    let ended = events::follow(
        &client,
        &format!("{base}/api/events"),
        &[],
        &tell,
        events::SILENCE,
        &tokio::sync::watch::channel(false).0,
    )
    .await;
    drop(tell);

    let mut events = Vec::new();
    while let Some(changed) = heard.recv().await {
        events.push(changed);
    }
    (ended, events)
}

#[tokio::test]
async fn events_arrive_and_a_close_ends_the_subscription() {
    let base = serving(vec![
        stream_head(),
        event("queue-depth"),
        event("throughput"),
        Act::Close,
    ])
    .await;

    let (ended, heard) = follow(&base).await;

    assert_eq!(ended, Ended::Closed);
    assert_eq!(
        heard.iter().map(|e| e.panel.as_str()).collect::<Vec<_>>(),
        vec!["queue-depth", "throughput"]
    );
}

#[tokio::test]
async fn an_event_split_across_chunks_still_arrives() {
    // The ordinary case on a real network, and the one a parser that assumed a
    // chunk was an event would silently drop.
    let base = serving(vec![
        stream_head(),
        Act::Say("event: changed\ndata: {\"pan".to_string()),
        Act::Say("el\":\"queue-depth\"}\n\n".to_string()),
        Act::Close,
    ])
    .await;

    let (_, heard) = follow(&base).await;
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].panel, "queue-depth");
}

#[tokio::test]
async fn a_stream_that_is_not_a_stream_is_refused_rather_than_read() {
    // A platform whose events route has been replaced by an error document or a
    // login page. Read as a stream it would be an endless supply of nothing,
    // and the shell would believe it was subscribed.
    let base = serving(vec![Act::Head(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}"
            .to_string(),
    )])
    .await;

    let (ended, heard) = follow(&base).await;

    assert!(matches!(ended, Ended::NotAStream(_)), "{ended:?}");
    assert!(heard.is_empty());
}

#[tokio::test]
async fn a_refusal_is_not_a_subscription() {
    let base = serving(vec![Act::Head(
        "HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n".to_string(),
    )])
    .await;

    let (ended, _) = follow(&base).await;
    assert!(matches!(ended, Ended::Refused(_)), "{ended:?}");
}

#[tokio::test]
async fn nobody_watching_ends_the_subscription() {
    // The subscription lifecycle in one line: the connection lives exactly as
    // long as somebody holds the other end, so a shell watching nothing holds
    // nothing open.
    let base = serving(vec![
        stream_head(),
        event("a"),
        Act::Wait(Duration::from_millis(50)),
        event("b"),
        Act::HoldForever,
    ])
    .await;

    let (tell, heard) = mpsc::channel(1);
    let client = reqwest::Client::new();

    // Dropped immediately: nobody is listening at all.
    drop(heard);

    let ended = tokio::time::timeout(
        Duration::from_secs(5),
        events::follow(
            &client,
            &format!("{base}/api/events"),
            &[],
            &tell,
            events::SILENCE,
            &tokio::sync::watch::channel(false).0,
        ),
    )
    .await
    .expect("it must notice without waiting for the silence timeout");

    assert_eq!(ended, Ended::NobodyWatching);
}

#[tokio::test]
async fn a_stream_that_goes_quiet_without_closing_is_noticed() {
    // The failure this whole heartbeat mechanism exists for, and the only one
    // here with no signal of its own: the socket is open, the platform is
    // reachable, and nothing will ever arrive again. Without a read timeout the
    // shell would hold it forever, believing it was subscribed, polling less on
    // a promise nobody is keeping — the one way this feature could leave a
    // viewer worse off than plain polling.
    let base = serving(vec![stream_head(), event("queue-depth"), Act::HoldForever]).await;

    let (tell, mut heard) = mpsc::channel(16);
    let client = reqwest::Client::new();

    let started = std::time::Instant::now();
    let ended = events::follow(
        &client,
        &format!("{base}/api/events"),
        &[],
        &tell,
        // A short patience, so this test is fast enough to keep. The mechanism
        // is identical at forty seconds.
        Duration::from_millis(300),
        &tokio::sync::watch::channel(false).0,
    )
    .await;

    assert_eq!(ended, Ended::WentQuiet);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it must give up on its own rather than wait for something that is not coming"
    );

    // And what did arrive before the silence was still delivered. A dead stream
    // does not retract what it already said.
    assert_eq!(
        heard.recv().await.map(|e| e.panel),
        Some("queue-depth".into())
    );
}

#[tokio::test]
async fn a_platform_that_vanishes_mid_stream_ends_the_subscription() {
    // A process that died rather than one that closed politely: the body is
    // truncated with no terminating chunk. Different from a clean close, and
    // the caller does the same thing about both — reconnect, and poll at the
    // declared cadence until it works.
    let base = serving(vec![stream_head(), event("queue-depth"), Act::Vanish]).await;

    let (ended, heard) = follow(&base).await;

    assert!(
        matches!(ended, Ended::Refused(_) | Ended::Closed),
        "a truncated stream ends the subscription rather than hanging: {ended:?}"
    );
    assert_eq!(
        heard.len(),
        1,
        "and what arrived before it died still counts"
    );
}

#[tokio::test]
async fn an_endless_event_is_refused_rather_than_held() {
    // A held-open response is a body that never ends, so the shell's ordinary
    // rule — read at most this much, then refuse — cannot apply to the
    // response. It applies to one event. Without this, a platform that opens a
    // stream and sends one endless line decides how much memory the shell uses.
    let mut script = vec![stream_head()];
    for _ in 0..40 {
        // No blank line anywhere: this is one event that never finishes.
        script.push(Act::Say("data: ".to_string() + &"x".repeat(4096)));
    }
    script.push(Act::HoldForever);

    let base = serving(script).await;
    let (ended, heard) = follow(&base).await;

    assert_eq!(ended, Ended::TooMuch);
    assert!(heard.is_empty());
}
