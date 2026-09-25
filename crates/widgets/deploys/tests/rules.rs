//! The deploy log, read the way the shell reads it: one long streamed read.

use std::time::Duration;

use chrono::Utc;
use hlin_widget_deploys::{Deploys, Line, PANEL, RECENT, api, line, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};

/// Fast enough that a test sees lines arrive, slow enough to tell apart.
const TICK: Duration = Duration::from_millis(50);

async fn deploys() -> (Running, Deploys) {
    let log = Deploys::every(TICK);
    (testing::start(widget(), log.clone(), api()).await, log)
}

/// The log, held open as the shell holds it: a read, with the viewer's token.
async fn open(running: &Running, query: &str) -> reqwest::Response {
    let token = running.read_token(&person("u-alice", "Alice"));
    reqwest::Client::new()
        .get(format!("{}/api/deploys/log{query}", running.base))
        .header("x-hlin-identity", token)
        .send()
        .await
        .expect("the widget answers")
}

/// Reads whole lines off `body` until there are `count` of them, however the
/// chunks fall, as the module does.
async fn lines(body: &mut reqwest::Response, count: usize) -> Vec<Line> {
    let mut seen = Vec::new();
    let mut partial = String::new();
    while seen.len() < count {
        let chunk = tokio::time::timeout(Duration::from_secs(5), body.chunk())
            .await
            .expect("a line within five seconds")
            .expect("the body reads")
            .expect("the log does not end by itself");
        partial.push_str(std::str::from_utf8(&chunk).expect("UTF-8"));
        while let Some(end) = partial.find('\n') {
            let text: String = partial.drain(..=end).collect();
            seen.push(serde_json::from_str(text.trim_end()).expect("a line is JSON"));
        }
    }
    seen
}

async fn until_nobody_is_reading(log: &Deploys) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while log.open_readers() > 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the platform kept writing to a reader who had gone"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_recent_lines_as_a_table_asked_for_again_every_few_seconds() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert_eq!(panel.envelope.as_deref(), Some("records.v1"));
    assert_eq!(panel.refresh_ms, Some(5_000));
    assert!(!panel.pushed, "nobody writes to the log");
}

#[test]
fn a_line_is_the_same_for_everybody_every_time() {
    let at = Utc::now();
    assert_eq!(line(4242, at), line(4242, at));
    // Four lines to a deploy, all about the same service and version.
    let deploy: Vec<Line> = (400..404).map(|seq| line(seq, at)).collect();
    assert!(deploy.iter().all(|line| line.service == deploy[0].service));
    assert!(deploy.iter().all(|line| line.version == deploy[0].version));
    assert!(deploy[0].text.starts_with("deploy to "));
    assert!(matches!(deploy[3].level.as_str(), "good" | "bad"));
}

#[test]
fn a_reader_starts_with_the_recent_lines_or_after_the_one_it_has() {
    let log = Deploys::every(Duration::from_secs(1));
    let now = Utc::now();
    let latest = log.latest(now);
    assert_eq!(log.first_for(None, now), latest + 1 - RECENT);
    assert_eq!(log.first_for(Some(latest - 2), now), latest - 1);
    // Far behind is the recent lines, not an hour of catching up.
    assert_eq!(log.first_for(Some(latest - 3600), now), latest + 1 - RECENT);
    // Ahead of the log is the next line due, not a wait for the future.
    assert_eq!(log.first_for(Some(latest + 99), now), latest + 1);
}

#[tokio::test]
async fn the_log_is_streamed_the_recent_lines_then_each_as_it_is_due() {
    let (running, log) = deploys().await;
    let mut body = open(&running, "").await;
    assert_eq!(body.status(), 200);
    assert_eq!(
        body.headers()["content-type"].to_str().unwrap(),
        "application/x-ndjson"
    );
    assert_eq!(log.open_readers(), 1);

    let got = lines(&mut body, RECENT as usize + 4).await;
    for pair in got.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1, "in order, with no gaps");
    }
    // The last few were written as they fell due, not all at once.
    let live = &got[RECENT as usize..];
    assert!(live.last().unwrap().at > live[0].at);
    assert!(Utc::now() >= live.last().unwrap().at);
}

#[tokio::test]
async fn a_reader_that_says_where_it_got_to_carries_on_from_there() {
    let (running, log) = deploys().await;
    let after = log.latest(Utc::now()) - 3;
    let mut body = open(&running, &format!("?after={after}")).await;
    let got = lines(&mut body, 2).await;
    assert_eq!(got[0].seq, after + 1);
    assert_eq!(got[1].seq, after + 2);
}

#[tokio::test]
async fn the_log_stops_when_its_reader_goes() {
    let (running, log) = deploys().await;
    let mut first = open(&running, "").await;
    let mut second = open(&running, "").await;
    lines(&mut first, 2).await;
    lines(&mut second, 2).await;
    assert_eq!(log.open_readers(), 2);

    // The shell letting go of one connection, as it does when a module
    // cancels, is unmounted, or its page closes.
    drop(first);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while log.open_readers() > 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the first reader's log kept running"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The other reader is untouched.
    lines(&mut second, 3).await;

    drop(second);
    until_nobody_is_reading(&log).await;
}

#[tokio::test]
async fn only_someone_signed_in_may_read_the_log() {
    let (running, log) = deploys().await;
    let refused = running
        .send("GET", "/api/deploys/log", None, None, None)
        .await;
    assert_eq!(refused.status, 401);
    assert_eq!(log.open_readers(), 0, "no log was started for nobody");
}

#[tokio::test]
async fn nobody_may_write_to_the_log() {
    let (running, _) = deploys().await;
    let alice = person("u-alice", "Alice");
    let refused = running
        .write(&alice, "POST", "/api/deploys/log", None)
        .await;
    assert_eq!(refused.status, 405);
}

#[tokio::test]
async fn the_fallback_is_the_recent_lines_newest_first() {
    let (running, log) = deploys().await;
    let Envelope::Records(table) = running.fallback(&person("u-alice", "Alice")).await else {
        panic!("the log's fallback is records");
    };
    assert_eq!(table.rows.len(), RECENT as usize);
    let texts: Vec<_> = table.rows.iter().map(|row| row["text"].clone()).collect();
    let newest = log.latest(Utc::now());
    // Allow for lines falling due between the two readings.
    let expected = |latest: u64| -> Vec<serde_json::Value> {
        (latest + 1 - RECENT..=latest)
            .rev()
            .map(|seq| log.line(seq).text.into())
            .collect()
    };
    assert!((0..20).any(|back| texts == expected(newest - back)));
}
