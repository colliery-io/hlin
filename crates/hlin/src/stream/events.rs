//! Listening to a platform say that something changed.
//!
//! The shell's side of specification HLIN-S-0006. One connection per platform,
//! held open, carrying nothing but the news that a panel's data has moved.
//!
//! Everything here is best-effort, and that is a design property rather than a
//! shortcut. Polling continues underneath at a relaxed interval (decision
//! HLIN-A-0011), so a notification that never arrives, arrives late, or is
//! dropped on the floor costs one interval of staleness and never costs
//! correctness. That is what lets this module have no acknowledgements, no
//! replay, no ordering and no delivery state to reconcile — and it is why every
//! failure below is handled by giving up and reconnecting rather than by trying
//! to recover anything.
//!
//! The one failure with no signal of its own is a connection that stops
//! delivering without closing: a NAT table forgets it, a proxy drops what looks
//! idle, a platform process wedges. Nothing announces any of those, so a
//! heartbeat is required of the platform and its absence is treated as a drop.
//! Without that the shell would hold a socket it believes is live and poll less
//! on a promise nobody is keeping, which is the one way this feature could leave
//! a viewer worse off than plain polling.

use std::collections::BTreeMap;
use std::time::Duration;

use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;

/// The event type a platform sends. Anything else is ignored.
const CHANGED: &str = "changed";

/// Who the shell is, to a platform it subscribes to.
///
/// A platform verifying the shell's token sees this as the subject. It is the
/// shell rather than any viewer on purpose: the stream carries no data, so
/// there is nothing in it that could be one person's.
const SHELL_PRINCIPAL: &str = "hlin:shell";

/// How long to wait before subscribing again after a stream ends.
///
/// A constant rather than the panel retry schedule, because the two are about
/// different things: a panel backs off so a struggling platform is not asked
/// harder, and this backs off so a platform that cannot hold a connection open
/// is not asked to hold one every second. Every panel is on its declared
/// cadence throughout, so there is nothing a viewer can see going wrong while
/// this waits.
pub const RESUBSCRIBE_AFTER: Duration = Duration::from_secs(30);

/// How long the shell will wait for anything at all before calling the
/// connection dead.
///
/// Twice the heartbeat interval HLIN-S-0006 requires, so a platform that emits
/// on time is never mistaken for one that has stopped, and one that has stopped
/// is noticed within a minute.
///
/// Passed in rather than read here, for the reason the aggregator takes a clock
/// rather than calling `Utc::now`: a test of the one failure with no signal of
/// its own should not take forty seconds to find out, and a test that slow is
/// one somebody eventually deletes.
pub const SILENCE: Duration = Duration::from_secs(40);

/// The most one event may be.
///
/// A held-open response is a body that never ends, so the shell's usual rule —
/// read at most this many bytes, then refuse ([[HLIN-T-0029]]) — cannot be
/// applied to the response. It applies to a single event instead, and to the
/// buffer holding an event that has not finished arriving. Without this, a
/// platform that opens a stream and sends one endless line decides how much
/// memory the shell uses.
const MOST_PER_EVENT: usize = 64 * 1024;

/// A platform saying one of its panels has changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The panel key, as that platform's manifest names it.
    pub panel: String,

    /// Which instances of it, or none to mean all of them.
    ///
    /// Matching is by subset: every key named here must match on an instance,
    /// and keys not named are not consulted. So the empty case falls out of the
    /// rule rather than needing one of its own — an event naming no selections
    /// reaches every instance of the panel.
    pub selections: BTreeMap<String, Vec<String>>,
}

impl Changed {
    /// Whether this event is about a panel instance with these selections.
    pub fn matches(&self, instance: &BTreeMap<String, Vec<String>>) -> bool {
        self.selections
            .iter()
            .all(|(param, wanted)| instance.get(param) == Some(wanted))
    }
}

/// What a platform puts in an event's `data`.
#[derive(Debug, Deserialize)]
struct Payload {
    panel: String,
    #[serde(default)]
    selections: BTreeMap<String, Vec<String>>,
}

/// Why a subscription ended.
///
/// Every variant means the same thing to the caller — reconnect, and poll at the
/// declared cadence until it works — so this exists for the log rather than for
/// a decision. An operator reading "the platform closed it" and one reading "it
/// went quiet without closing" are looking for different problems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// The platform could not be reached, or refused.
    Refused(String),
    /// It answered, but not with an event stream.
    NotAStream(String),
    /// It closed the connection.
    Closed,
    /// It stopped sending anything at all, including heartbeats.
    WentQuiet,
    /// It sent more in one event than the shell will hold.
    TooMuch,
    /// Nobody is listening any more, so there is no reason to hold it open.
    NobodyWatching,
}

impl std::fmt::Display for Ended {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(why) => write!(formatter, "could not subscribe: {why}"),
            Self::NotAStream(what) => {
                write!(formatter, "answered with {what}, not an event stream")
            }
            Self::Closed => formatter.write_str("the platform closed the stream"),
            Self::WentQuiet => formatter.write_str("the stream stopped delivering without closing"),
            Self::TooMuch => formatter.write_str("one event was larger than the shell will hold"),
            Self::NobodyWatching => formatter.write_str("nobody is watching this platform"),
        }
    }
}

/// Follow one platform's event stream until it ends.
///
/// Returns why it ended rather than reconnecting itself, so the caller owns the
/// backoff and can be tested without one.
///
/// Ends of its own accord when `tell` is closed — which is the subscription
/// lifecycle in one line: the connection lives exactly as long as somebody is
/// holding the other end, so a shell watching nothing holds nothing open
/// (REQ-2.1).
pub async fn follow(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    tell: &mpsc::Sender<Changed>,
    silence: Duration,
    connected: &tokio::sync::watch::Sender<bool>,
) -> Ended {
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(name, value);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return Ended::Refused(error.to_string()),
    };

    if !response.status().is_success() {
        return Ended::Refused(format!("answered {}", response.status()));
    }

    // A platform that answers an event-stream request with JSON is one whose
    // route has been replaced by something else — most often a login page or an
    // error document, both of which would otherwise be parsed as an endless
    // stream of nothing.
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("nothing")
        .to_string();
    if !content_type.starts_with("text/event-stream") {
        return Ended::NotAStream(content_type);
    }

    // Connected, and said so before anything has arrived — which is the point.
    // A stream that is healthy and has nothing to report looks identical to one
    // that is dead, so waiting for a first event to believe in the subscription
    // would leave every panel on its fast cadence against a platform reporting
    // perfectly well.
    let _ = connected.send(true);

    let mut body = response.bytes_stream();
    let mut unfinished = String::new();
    let mut behind = 0u64;

    loop {
        // The whole point of the heartbeat: this is the only thing that
        // distinguishes a healthy quiet stream from a dead one, because a dead
        // one looks exactly like a quiet one from here.
        let chunk = match tokio::time::timeout(silence, body.next()).await {
            Err(_) => return Ended::WentQuiet,
            Ok(None) => return Ended::Closed,
            Ok(Some(Err(error))) => return Ended::Refused(error.to_string()),
            Ok(Some(Ok(chunk))) => chunk,
        };

        unfinished.push_str(&String::from_utf8_lossy(&chunk));

        if unfinished.len() > MOST_PER_EVENT {
            return Ended::TooMuch;
        }

        for changed in take_events(&mut unfinished) {
            match tell.try_send(changed) {
                Ok(()) => {}

                // Dropped rather than waited on, and the difference matters.
                // Blocking here would stop reading the socket, which pushes
                // back on the platform — making the shell's own consumption
                // rate a platform's problem, which it is not. And an event is
                // news the shell is free to miss: the poll underneath means the
                // worst case is one relaxed interval of staleness, which is
                // strictly better than holding a platform's write path open.
                Err(mpsc::error::TrySendError::Full(_)) => {
                    behind += 1;
                }

                // The ordinary way this ends: the last surface watching this
                // platform stopped.
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return Ended::NobodyWatching;
                }
            }
        }

        // Said once per stream rather than per event, because a platform that
        // floods would otherwise flood the log too.
        if behind > 0 && behind.is_power_of_two() {
            tracing::debug!(behind, "dropping events faster than they can be applied");
        }
    }
}

/// Take every complete event out of the buffer, leaving what has not arrived.
///
/// Separate from the connection so that the whole of the parsing — which is
/// where a platform's malformed output gets to be a problem — is a pure
/// function of a string.
fn take_events(buffer: &mut String) -> Vec<Changed> {
    let mut found = Vec::new();

    // SSE separates events with a blank line. Anything before the last one is
    // complete; anything after it is still arriving.
    while let Some(end) = buffer.find("\n\n") {
        let block: String = buffer.drain(..end + 2).collect();
        if let Some(changed) = read_event(&block) {
            found.push(changed);
        }
    }

    found
}

/// One event block, if it is a `changed` event the shell can read.
///
/// Everything unreadable is dropped rather than closing the stream: a comment,
/// an event type this shell does not know, a `data` line that is not JSON, or
/// JSON without a panel. A platform mid-deploy may serve a stream from a newer
/// revision than the manifest the shell last read, and a shell that hung up on
/// anything it did not recognise would make every platform's deploy window an
/// outage of its own event stream.
fn read_event(block: &str) -> Option<Changed> {
    let mut kind: Option<&str> = None;
    let mut data = String::new();

    for line in block.lines() {
        // A comment. This is what a heartbeat is, and it carries no meaning
        // beyond having arrived — which is the meaning that matters.
        if line.starts_with(':') {
            continue;
        }

        // The blank line that ends a block, and any line without a colon, which
        // the format defines as a field with an empty value. Skipped rather
        // than fatal — and the difference is not academic: reading these as a
        // failure of the whole event meant *every* well-formed event was
        // discarded, because every block ends with one.
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.strip_prefix(' ').unwrap_or(value);

        match field {
            "event" => kind = Some(value),
            // Several data lines are one payload, joined by newlines.
            "data" => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
            _ => {}
        }
    }

    if kind != Some(CHANGED) {
        return None;
    }

    let payload: Payload = serde_json::from_str(&data).ok()?;
    if payload.panel.trim().is_empty() {
        return None;
    }

    Some(Changed {
        panel: payload.panel,
        selections: payload.selections,
    })
}

/// Follow a platform's event stream forever, reconnecting when it ends.
///
/// The backoff is the shell's configured retry schedule rather than one
/// invented here: a platform that will not hold a stream open is a platform
/// having a problem, and there is no reason for the shell to have a second
/// opinion about how often to bother it.
///
/// Returns when nobody is listening any more, which is the only ending that is
/// not a failure.
pub async fn follow_forever(
    client: reqwest::Client,
    platform_id: String,
    url: String,
    headers: Vec<(String, String)>,
    tell: mpsc::Sender<Changed>,
    from: Duration,
    ceiling: Duration,
) {
    let mut wait = from;

    loop {
        let (connected, _) = tokio::sync::watch::channel(false);
        let ended = follow(&client, &url, &headers, &tell, SILENCE, &connected).await;

        if matches!(ended, Ended::NobodyWatching) {
            tracing::debug!(platform = platform_id, "stopped following: {ended}");
            return;
        }

        // Deliberately not an error. A platform without an event stream is the
        // ordinary case this whole design is built to stay correct under, and a
        // platform whose stream is down is that case temporarily. Every panel is
        // back on its declared cadence while this is true, so nothing a viewer
        // can see is wrong — which is why it is worth a line and not an alarm.
        tracing::info!(
            platform = platform_id,
            "not receiving events, polling instead: {ended}"
        );

        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(ceiling);
    }
}

/// The principal the shell subscribes as.
///
/// An event stream is per platform, not per viewer: it carries no data, so
/// there is nothing in it that could be one person's and not another's. The
/// shell therefore subscribes as itself, once, and every viewer of that
/// platform benefits from the one connection.
///
/// That has a consequence worth stating rather than discovering. A platform
/// whose credential strategy is `forward-session` cannot be subscribed to at
/// all, because the shell has no session of its own to forward — and it must
/// not borrow a viewer's, which would make one person's cookie the credential
/// for a connection serving everybody. Such a platform is polled, which is the
/// behaviour it had before this feature existed and is exactly the degradation
/// HLIN-A-0011 requires.
pub fn shell_itself() -> crate::identity::Viewer {
    let mut principal = hlin_identity::Principal::new(SHELL_PRINCIPAL.to_string());
    principal.name = Some("Hlin".to_string());
    crate::identity::Viewer::new(principal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(panel: &str) -> Changed {
        Changed {
            panel: panel.to_string(),
            selections: BTreeMap::new(),
        }
    }

    #[test]
    fn a_complete_event_is_read_and_a_partial_one_waits() {
        let mut buffer = String::from("event: changed\ndata: {\"panel\":\"queue-depth\"}\n\n");
        assert_eq!(take_events(&mut buffer), vec![changed("queue-depth")]);
        assert!(buffer.is_empty());

        // Half an event, which is the ordinary case at a chunk boundary.
        buffer.push_str("event: changed\ndata: {\"pan");
        assert_eq!(take_events(&mut buffer), vec![]);
        assert!(!buffer.is_empty(), "the rest is still expected");

        buffer.push_str("el\":\"throughput\"}\n\n");
        assert_eq!(take_events(&mut buffer), vec![changed("throughput")]);
    }

    #[test]
    fn several_events_in_one_chunk_all_arrive() {
        let mut buffer = String::from(
            "event: changed\ndata: {\"panel\":\"a\"}\n\n\
             event: changed\ndata: {\"panel\":\"b\"}\n\n",
        );
        assert_eq!(take_events(&mut buffer), vec![changed("a"), changed("b")]);
    }

    #[test]
    fn a_heartbeat_carries_no_news() {
        // It means only that the platform is there, which is the whole of its
        // job. Reading it as an event would refetch every panel every fifteen
        // seconds and quietly undo the saving this feature exists for.
        let mut buffer = String::from(":heartbeat\n\n");
        assert_eq!(take_events(&mut buffer), vec![]);
    }

    #[test]
    fn a_platform_saying_something_unreadable_is_ignored_not_fatal() {
        // Each of these is a real platform mid-deploy or mid-mistake, and none
        // of them should cost the shell its subscription.
        for block in [
            "event: something-else\ndata: {\"panel\":\"a\"}\n\n",
            "event: changed\ndata: not json at all\n\n",
            "event: changed\ndata: {\"no\":\"panel\"}\n\n",
            "event: changed\ndata: {\"panel\":\"\"}\n\n",
            "event: changed\n\n",
            "\n\n",
        ] {
            let mut buffer = String::from(block);
            assert_eq!(take_events(&mut buffer), vec![], "should ignore: {block:?}");
            assert!(buffer.is_empty(), "and consume it: {block:?}");
        }
    }

    #[test]
    fn selections_narrow_an_event_by_subset() {
        let mut buffer = String::from(
            "event: changed\ndata: {\"panel\":\"t\",\"selections\":{\"cluster\":[\"west\"]}}\n\n",
        );
        let event = take_events(&mut buffer).remove(0);

        let mut west = BTreeMap::new();
        west.insert("cluster".to_string(), vec!["west".to_string()]);
        assert!(event.matches(&west));

        // Keys the event did not mention are not consulted, which is what makes
        // a platform able to say "west changed" without enumerating every
        // combination of every other control a viewer might have set.
        let mut west_and_more = west.clone();
        west_and_more.insert("tier".to_string(), vec!["gold".to_string()]);
        assert!(event.matches(&west_and_more));

        let mut east = BTreeMap::new();
        east.insert("cluster".to_string(), vec!["east".to_string()]);
        assert!(!event.matches(&east));

        assert!(
            !event.matches(&BTreeMap::new()),
            "an instance that has not chosen a cluster is not the west one"
        );
    }

    #[test]
    fn an_event_naming_no_selections_is_about_every_instance() {
        // The empty case, which falls out of the subset rule rather than being
        // special-cased.
        let event = changed("t");
        assert!(event.matches(&BTreeMap::new()));

        let mut anything = BTreeMap::new();
        anything.insert("cluster".to_string(), vec!["east".to_string()]);
        assert!(event.matches(&anything));
    }
}
