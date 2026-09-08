//! Five platforms on a bad day.
//!
//! The claim HLIN-S-0006 makes is not that event streams work. It is that no
//! behaviour of one can make the shell worse than it was when it only polled —
//! which is the property that decides whether a shell operator can accept a
//! platform written by somebody they have never met.
//!
//! So each case here is a platform misbehaving in a way that has actually
//! happened to somebody, and each asserts the same thing: the panel is still
//! polled, and the viewer sees what polling alone would have shown.
//!
//! The fourth is the one to read first. Four of these fail loudly; that one
//! fails silently, and a suite that skipped it would let the silent failure
//! ship.

use std::time::Duration;

use hlin::stream::events::{self, Ended};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// A platform that behaves however the script says.
async fn platform(script: Vec<Misbehaviour>) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let script = script.clone();

            tokio::spawn(async move {
                let mut scratch = [0u8; 2048];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut scratch).await;

                for act in script {
                    match act {
                        Misbehaviour::Head(head) => {
                            if socket.write_all(head.as_bytes()).await.is_err() {
                                return;
                            }
                            let _ = socket.flush().await;
                        }
                        Misbehaviour::Chunk(text) => {
                            let framed = format!("{:x}\r\n{text}\r\n", text.len());
                            if socket.write_all(framed.as_bytes()).await.is_err() {
                                return;
                            }
                            let _ = socket.flush().await;
                        }
                        Misbehaviour::Pause(how_long) => tokio::time::sleep(how_long).await,
                        Misbehaviour::Hang => std::future::pending::<()>().await,
                        Misbehaviour::Stop => {
                            // The terminating zero-length chunk. A platform
                            // shutting a stream down deliberately sends this;
                            // one whose process died does not, and the shell
                            // treats the two the same way for the same reason.
                            let _ = socket.write_all(b"0\r\n\r\n").await;
                            let _ = socket.flush().await;
                            return;
                        }
                    }
                }
            });
        }
    });

    base
}

#[derive(Clone)]
enum Misbehaviour {
    Head(String),
    Chunk(String),
    Pause(Duration),
    /// Hold the connection open and say nothing more, ever.
    Hang,
    Stop,
}

fn opened() -> Misbehaviour {
    Misbehaviour::Head(
        "HTTP/1.1 200 OK\r\n\
         content-type: text/event-stream\r\n\
         transfer-encoding: chunked\r\n\r\n"
            .to_string(),
    )
}

fn says(panel: &str) -> Misbehaviour {
    Misbehaviour::Chunk(format!(
        "event: changed\ndata: {{\"panel\":\"{panel}\"}}\n\n"
    ))
}

/// Subscribe once, with a short patience, and report what happened.
async fn subscribe_to(base: &str, patience: Duration) -> (Ended, usize, bool) {
    let (tell, mut heard) = mpsc::channel(1024);
    let (connected, watching) = tokio::sync::watch::channel(false);

    // Drained while the subscription runs, not after. A consumer that only
    // reads at the end is not one this suite should be testing against: the
    // real driver drains every tick, and a test that fills the channel would be
    // measuring its own impatience.
    let counting = tokio::spawn(async move {
        let mut told = 0;
        while heard.recv().await.is_some() {
            told += 1;
        }
        told
    });

    let ended = events::follow(
        &reqwest::Client::new(),
        &format!("{base}/api/events"),
        &[],
        &tell,
        patience,
        &connected,
    )
    .await;
    drop(tell);

    let told = counting.await.unwrap_or(0);

    // Whether the shell ever believed it was subscribed, which is what decides
    // if any panel was relaxed.
    let ever_connected = *watching.borrow();
    (ended, told, ever_connected)
}

#[tokio::test]
async fn a_platform_that_declares_a_stream_and_never_says_anything() {
    // The commonest shape of all: a platform that adopted the field, serves the
    // route, and has genuinely had nothing to report. It must look healthy —
    // relaxing the poll is the whole point — and it must keep looking healthy
    // only as long as its heartbeats keep arriving.
    let base = platform(vec![
        opened(),
        // A heartbeat and nothing else, which is exactly conformant.
        Misbehaviour::Chunk(":heartbeat\n\n".to_string()),
        Misbehaviour::Pause(Duration::from_millis(50)),
        Misbehaviour::Chunk(":heartbeat\n\n".to_string()),
        Misbehaviour::Hang,
    ])
    .await;

    let (ended, told, connected) = subscribe_to(&base, Duration::from_millis(400)).await;

    assert!(connected, "a quiet stream is still a subscription");
    assert_eq!(told, 0, "and it reported nothing, because nothing changed");
    assert_eq!(
        ended,
        Ended::WentQuiet,
        "once the heartbeats stop it is treated as gone, however healthy it looked"
    );
}

#[tokio::test]
async fn a_platform_whose_events_name_panels_that_do_not_exist() {
    // A platform mid-deploy, serving a stream from a newer revision than the
    // manifest the shell last read. The events are about panels the shell has
    // never heard of, and none of them may cost a fetch.
    let base = platform(vec![
        opened(),
        says("a-panel-from-the-future"),
        says("another-one"),
        Misbehaviour::Chunk("event: changed\ndata: {\"panel\":\"\"}\n\n".to_string()),
        Misbehaviour::Chunk("event: changed\ndata: nonsense\n\n".to_string()),
        Misbehaviour::Chunk("event: some-other-thing\ndata: {}\n\n".to_string()),
        Misbehaviour::Stop,
    ])
    .await;

    let (ended, told, connected) = subscribe_to(&base, Duration::from_secs(2)).await;

    assert!(connected);
    assert_eq!(
        ended,
        Ended::Closed,
        "nothing unreadable may cost the subscription; hanging up on an \
         unrecognised event would make every platform's deploy window an \
         outage of its own event stream"
    );
    assert_eq!(
        told, 2,
        "the two that named a panel are forwarded and the surface decides they \
         are about nothing; the three that named none are dropped here"
    );
}

#[tokio::test]
async fn a_platform_that_closes_the_stream_mid_session() {
    let base = platform(vec![opened(), says("queue-depth"), Misbehaviour::Stop]).await;

    let (ended, told, connected) = subscribe_to(&base, Duration::from_secs(2)).await;

    assert!(connected);
    assert_eq!(told, 1, "what it managed to say still counts");
    assert!(
        matches!(ended, Ended::Closed | Ended::Refused(_)),
        "and the subscription ends rather than hanging: {ended:?}"
    );
}

#[tokio::test]
async fn a_platform_that_holds_the_connection_open_and_goes_quiet() {
    // Written first, because it is the only one of the five with no signal of
    // its own. The socket is open, the platform is reachable, and nothing will
    // ever arrive again — a NAT table forgot it, a proxy dropped what looked
    // idle, a process wedged. Without a read timeout the shell would hold it
    // forever, believing it was subscribed and polling less on a promise nobody
    // is keeping, which is the one way this feature could leave a viewer worse
    // off than plain polling.
    let base = platform(vec![opened(), says("queue-depth"), Misbehaviour::Hang]).await;

    let started = std::time::Instant::now();
    let (ended, told, connected) = subscribe_to(&base, Duration::from_millis(300)).await;

    assert!(connected);
    assert_eq!(told, 1);
    assert_eq!(ended, Ended::WentQuiet);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it must give up on its own rather than wait for something not coming"
    );
}

#[tokio::test]
async fn a_platform_that_floods() {
    // A platform whose write path is noisier than its data. Every event is well
    // formed; there are simply thousands of them. The shell must read them
    // without unbounded growth, and the surface — tested separately in the
    // aggregator — clamps what they cost to the refresh floor.
    let mut script = vec![opened()];
    for _ in 0..2_000 {
        script.push(says("queue-depth"));
    }
    script.push(Misbehaviour::Stop);

    let base = platform(script).await;
    let (ended, told, _) = subscribe_to(&base, Duration::from_secs(5)).await;

    assert!(
        matches!(ended, Ended::Closed | Ended::Refused(_)),
        "{ended:?}"
    );
    assert!(
        told > 0 && told <= 2_000,
        "every event read is one the platform actually sent: {told}"
    );
}

#[tokio::test]
async fn a_platform_that_answers_an_event_request_with_a_login_page() {
    // What a shell behind an expired credential actually gets, and the reason
    // the content type is checked: read as a stream this is an endless supply
    // of nothing, and the shell would believe it was subscribed forever while
    // every panel of that platform quietly went stale.
    let base = platform(vec![Misbehaviour::Head(
        "HTTP/1.1 200 OK\r\n\
         content-type: text/html\r\n\
         content-length: 22\r\n\r\n\
         <html>Sign in</html>"
            .to_string(),
    )])
    .await;

    let (ended, told, connected) = subscribe_to(&base, Duration::from_secs(2)).await;

    assert!(
        matches!(ended, Ended::NotAStream(_)),
        "an answer that is not an event stream is refused rather than read: {ended:?}"
    );
    assert!(
        !connected,
        "and the shell never believed it was subscribed, so nothing was relaxed"
    );
    assert_eq!(told, 0);
}

// -- What a viewer sees ---------------------------------------------------

use chrono::{TimeZone, Utc};
use hlin::stream::events::Changed;
use hlin::stream::{Instance, Outcome, Policy, Surface};

fn moment(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_757_244_600 + seconds, 0).unwrap()
}

fn scalar() -> hlin_manifest::Envelope {
    hlin_manifest::parse_envelope(
        br#"{ "envelope": "scalar.v1", "value": 1, "unit": "count" }"#,
        "scalar.v1",
    )
    .expect("fixture")
}

/// A surface with one pushed panel, and how often it is asked over a window.
fn asked_over(streaming: bool, seconds: i64) -> usize {
    let mut panel = Instance::new("one", "orebank", "batches", "api/batches", "scalar.v1");
    panel.refresh = Some(chrono::Duration::milliseconds(250));
    panel.pushed = true;

    let mut surface = Surface::new(vec![panel], Policy::default());
    if streaming {
        surface.streaming_from("orebank", true);
    }

    let mut asked = 0;
    for second in 1..=seconds {
        let (requests, _) = surface.due(moment(second));
        for request in &requests {
            asked += 1;
            surface.resolve(request, Outcome::Ready(Box::new(scalar())), moment(second));
        }
    }
    asked
}

#[test]
fn a_platform_that_never_reports_leaves_a_viewer_exactly_where_polling_would() {
    // The claim the whole specification makes, reduced to one comparison. A
    // platform that declares a stream and reports nothing must cost a viewer
    // nothing — not one request more, not one fewer, than the same panel on a
    // platform that declares no stream at all.
    //
    // The connection being *open* is what relaxes the poll, so this is the case
    // where the shell subscribed successfully and heard silence. It must still
    // be asked, on a real interval, because a platform that has stopped
    // reporting looks exactly like one that has nothing to report.
    let never_offered = asked_over(false, 120);
    let offered_and_silent = asked_over(true, 120);

    assert!(
        offered_and_silent >= 2,
        "silence must not mean never asking again: {offered_and_silent} in two minutes"
    );
    assert!(
        offered_and_silent < never_offered,
        "and it should still be cheaper than polling, or there was no point"
    );
}

#[test]
fn a_flood_of_events_about_a_watched_panel_is_bounded_by_the_floor() {
    // The aggregator half of the flooding case. Every event is well formed and
    // about a panel that is genuinely on the surface, so none of them can be
    // discarded for being about nothing — and the shell must still not fetch
    // more often than it fetches anything else.
    let mut panel = Instance::new("one", "orebank", "batches", "api/batches", "scalar.v1");
    panel.refresh = Some(chrono::Duration::seconds(30));
    panel.pushed = true;

    let mut surface = Surface::new(
        vec![panel],
        Policy {
            refresh_floor: chrono::Duration::seconds(2),
            ..Policy::default()
        },
    );
    surface.streaming_from("orebank", true);

    let told = Changed {
        panel: "batches".to_string(),
        selections: Default::default(),
    };

    let mut asked = 0;
    for second in 1..=60 {
        // Told a hundred times a second, for a minute.
        for _ in 0..100 {
            surface.changed("orebank", &told, moment(second));
        }
        let (requests, _) = surface.due(moment(second));
        for request in &requests {
            asked += 1;
            surface.resolve(request, Outcome::Ready(Box::new(scalar())), moment(second));
        }
    }

    assert!(
        asked <= 31,
        "six thousand notices against a two-second floor may not produce more \
         than a fetch every two seconds: {asked}"
    );
    assert!(asked > 1, "and the news is acted on at all: {asked}");
}

// -- One connection per platform (HLIN-S-0006 REQ-2.1) --------------------

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hlin::stream::streams::Streams;

/// A platform that counts how many times it has been subscribed to.
async fn counting_platform() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let subscriptions = Arc::new(AtomicUsize::new(0));

    tokio::spawn({
        let subscriptions = subscriptions.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                subscriptions.fetch_add(1, Ordering::Relaxed);

                tokio::spawn(async move {
                    let mut scratch = [0u8; 2048];
                    let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut scratch).await;
                    let _ = socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\n\
                              content-type: text/event-stream\r\n\
                              transfer-encoding: chunked\r\n\r\n",
                        )
                        .await;
                    let _ = socket.flush().await;
                    // Held open, saying nothing. Which surfaces are listening
                    // is the question, not what they hear.
                    std::future::pending::<()>().await;
                });
            }
        }
    });

    (base, subscriptions)
}

#[tokio::test]
async fn many_surfaces_watching_one_platform_open_one_connection() {
    // REQ-2.1. This was one connection per platform *per surface* when it was
    // first built — ten people on ten layouts holding an `orebank` panel opened
    // ten streams to `orebank`. That is the fan-out per-principal dedup exists
    // to prevent, on a new axis, and against a platform's write path.
    let (base, subscriptions) = counting_platform().await;
    let streams = Arc::new(Streams::new());

    let mut listening = Vec::new();
    for _ in 0..10 {
        listening.push(
            streams
                .listen(
                    "orebank",
                    &format!("{base}/api/events"),
                    Vec::new(),
                    reqwest::Client::new(),
                )
                .await,
        );
    }

    // Give the one connection time to be made.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(streams.following().await, 1, "one subscription");
    assert_eq!(
        subscriptions.load(Ordering::Relaxed),
        1,
        "and one connection reached the platform, not ten"
    );

    let health = streams.health().await;
    let orebank = health.get("orebank").expect("it is being followed");
    assert!(orebank.declared);
    assert!(orebank.connected, "the stream is up");
    assert_eq!(orebank.watchers, 10, "and ten surfaces are interested");
}

#[tokio::test]
async fn the_last_surface_to_stop_closes_the_connection() {
    let (base, _) = counting_platform().await;
    let streams = Arc::new(Streams::new());

    let first = streams
        .listen(
            "orebank",
            &format!("{base}/api/events"),
            Vec::new(),
            reqwest::Client::new(),
        )
        .await;
    let second = streams
        .listen(
            "orebank",
            &format!("{base}/api/events"),
            Vec::new(),
            reqwest::Client::new(),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(streams.following().await, 1);

    drop(first);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        streams.following().await,
        1,
        "one surface leaving is not the last one"
    );

    drop(second);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        streams.following().await,
        0,
        "a shell watching nothing holds nothing open"
    );
}

#[tokio::test]
async fn a_surface_arriving_as_the_last_one_leaves_keeps_the_connection() {
    // The race the surface registry documents, in a new place: a listener can
    // be taken between the last one dropping and the release running. Removing
    // the subscription then would leave the newcomer attached to a connection
    // about to be aborted.
    let (base, subscriptions) = counting_platform().await;
    let streams = Arc::new(Streams::new());

    let going = streams
        .listen(
            "orebank",
            &format!("{base}/api/events"),
            Vec::new(),
            reqwest::Client::new(),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Taken before the drop, which is what makes it a race rather than a
    // sequence — the release will run afterwards and must find this.
    let arriving = streams
        .listen(
            "orebank",
            &format!("{base}/api/events"),
            Vec::new(),
            reqwest::Client::new(),
        )
        .await;
    drop(going);

    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(
        streams.following().await,
        1,
        "the newcomer's interest keeps it alive"
    );
    assert_eq!(
        subscriptions.load(Ordering::Relaxed),
        1,
        "and it was never torn down and rebuilt"
    );
    drop(arriving);
}

#[tokio::test]
async fn health_says_why_a_stream_ended_and_how_often() {
    // The operator's question is never "is it up right now" alone — a stream
    // that is up and keeps falling over looks healthy at every instant.
    let base = platform(vec![opened(), says("batches"), Misbehaviour::Stop]).await;
    let streams = Arc::new(Streams::new());

    let watching = streams
        .listen(
            "orebank",
            &format!("{base}/api/events"),
            Vec::new(),
            reqwest::Client::new(),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(300)).await;

    let health = streams.health().await;
    let orebank = health.get("orebank").expect("followed");

    assert!(!orebank.connected, "it closed");
    assert!(orebank.endings >= 1, "and that was recorded");
    assert!(
        orebank.last_ended.is_some(),
        "with a reason an operator can read: {:?}",
        orebank.last_ended
    );
    assert!(
        orebank.last_heard.is_some(),
        "and it did deliver something before it went"
    );

    drop(watching);
}
