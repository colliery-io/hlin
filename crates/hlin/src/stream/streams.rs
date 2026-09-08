//! One event stream per platform, shared by every surface that wants it.
//!
//! [[HLIN-S-0006]] REQ-2.1 says one connection per platform, held only while
//! some surface watches a panel that platform offers. The first implementation
//! subscribed inside the surface driver, which made it one connection per
//! platform *per surface*: ten people on ten layouts that each hold an
//! `orebank` panel opened ten event streams to `orebank`. That is the fan-out
//! per-principal deduplication exists to prevent ([[HLIN-A-0004]]),
//! reintroduced on a new axis, and against a platform's *write* path rather
//! than its read path.
//!
//! So subscriptions live here instead, in the shape [`crate::surfaces`] already
//! uses for surfaces themselves: a map, reference-counted by the handles it
//! hands out, with the connection ending when the last one goes.
//!
//! Health lives here too, and that is not incidental. An operator needs to know
//! that a platform's events have stopped, because a stream that has quietly
//! died is invisible from every other angle — the panels still draw, just less
//! currently than anybody believes. This is the only place that knows, so this
//! is where the answer is.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{Mutex, broadcast};

use super::events::{self, Changed, Ended};

/// How many events a slow surface may fall behind before it misses some.
///
/// A surface that far behind is better served by the poll underneath than by a
/// longer replay it will mostly discard — the same reasoning the browser stream
/// uses for its own buffer, for the same reason.
const BACKLOG: usize = 256;

/// What an operator can see about one platform's event stream.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StreamHealth {
    /// Whether this platform declares a stream at all.
    ///
    /// Reported separately from `connected` because they are different things
    /// and an operator should not have to infer one from the other: a platform
    /// that offers nothing is behaving exactly as intended, and one that offers
    /// a stream the shell cannot hold is not.
    pub declared: bool,

    /// Whether the shell is subscribed right now.
    ///
    /// The thing that decides whether any of this platform's panels are polled
    /// at the relaxed interval, so it is the number that matters.
    pub connected: bool,

    /// When the stream last delivered anything at all, heartbeats included.
    ///
    /// Absent means nothing has ever arrived. A timestamp drifting into the
    /// past while `connected` is true would mean the read timeout is not doing
    /// its job, which is worth being able to see.
    pub last_heard: Option<DateTime<Utc>>,

    /// Why the last attempt ended, when one has.
    ///
    /// Kept after reconnecting, because the interesting case is a stream that
    /// is up now and keeps falling over, which looks healthy at any instant.
    pub last_ended: Option<String>,

    /// How many times it has ended since the shell started.
    pub endings: u64,

    /// How many surfaces are currently interested.
    pub watchers: usize,
}

impl StreamHealth {
    /// A platform that offers no event stream.
    pub fn none_offered() -> Self {
        Self {
            declared: false,
            connected: false,
            last_heard: None,
            last_ended: None,
            endings: 0,
            watchers: 0,
        }
    }
}

/// What the registry knows about one platform, behind a lock.
#[derive(Debug, Default)]
struct Held {
    connected: bool,
    last_heard: Option<DateTime<Utc>>,
    last_ended: Option<String>,
    endings: u64,
}

/// One platform's subscription, and who is listening to it.
struct Running {
    events: broadcast::Sender<Changed>,
    /// Whether the platform is connected, watchable so a surface learns the
    /// moment it changes rather than on its next tick.
    connected: tokio::sync::watch::Sender<bool>,
    held: Arc<Mutex<Held>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A surface's interest in one platform's events.
///
/// Holding this is what keeps the connection open; dropping it is what
/// eventually closes it. Nothing else has to be remembered, which is the
/// property the per-surface version had for free and the shared version has to
/// be careful to keep.
pub struct Listening {
    /// Which platform this is an interest in.
    pub platform_id: String,
    /// The events themselves.
    pub events: broadcast::Receiver<Changed>,
    /// Whether the platform is connected, as it changes.
    pub connected: tokio::sync::watch::Receiver<bool>,
    /// Kept so the registry can count who is still interested.
    _interest: Arc<Interest>,
    /// Where to report that this listener has gone.
    home: std::sync::Weak<Streams>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        // The trigger belongs here rather than on `Interest`, and the first
        // version had it in the wrong place. The map holds a strong `Interest`
        // of its own, so `Interest::drop` could not run until it had already
        // been removed from the map — which is the thing it was supposed to
        // cause. Nothing ever closed, and the test that counts subscriptions
        // is what said so.
        //
        // Deferred to a task because `Drop` cannot await and the map is behind
        // an async lock. `release` re-checks the count under that lock, so the
        // deferral is safe: a listener taken in the meantime is seen.
        if let Some(home) = self.home.upgrade() {
            let platform_id = self._interest.platform_id.clone();
            tokio::spawn(async move { home.release(&platform_id).await });
        }
    }
}

/// A refcount the registry can observe without holding a listener.
///
/// Nothing but a counter: the map holds one and every listener holds one, so
/// the map's own is the only reference left when the last listener has gone.
struct Interest {
    platform_id: String,
}

/// Every event stream the shell is holding.
#[derive(Default)]
pub struct Streams {
    running: Mutex<BTreeMap<String, (Arc<Interest>, Running)>>,
}

impl Streams {
    /// Nothing subscribed to.
    pub fn new() -> Self {
        Self::default()
    }

    /// Listen to a platform's events, subscribing if nobody is yet.
    ///
    /// The lock is held across the whole of this for the reason
    /// [`crate::surfaces::Surfaces::for_surface`] holds its own: two surfaces
    /// opening together would otherwise both miss, both subscribe, and one
    /// connection would be left running with nobody able to find it — which is
    /// exactly the leak this task exists to remove, arrived at from the other
    /// direction.
    pub async fn listen(
        self: &Arc<Self>,
        platform_id: &str,
        url: &str,
        headers: Vec<(String, String)>,
        client: reqwest::Client,
    ) -> Listening {
        let mut running = self.running.lock().await;

        if let Some((interest, held)) = running.get(platform_id) {
            return Listening {
                platform_id: platform_id.to_string(),
                events: held.events.subscribe(),
                connected: held.connected.subscribe(),
                _interest: interest.clone(),
                home: Arc::downgrade(self),
            };
        }

        let (events, receiving) = broadcast::channel(BACKLOG);
        let (connected, watching) = tokio::sync::watch::channel(false);
        let held = Arc::new(Mutex::new(Held::default()));

        let task = tokio::spawn(follow_forever(
            client,
            platform_id.to_string(),
            url.to_string(),
            headers,
            events.clone(),
            connected.clone(),
            held.clone(),
        ));

        let interest = Arc::new(Interest {
            platform_id: platform_id.to_string(),
        });

        running.insert(
            platform_id.to_string(),
            (
                interest.clone(),
                Running {
                    events,
                    connected,
                    held,
                    task,
                },
            ),
        );

        Listening {
            platform_id: platform_id.to_string(),
            events: receiving,
            connected: watching,
            _interest: interest,
            home: Arc::downgrade(self),
        }
    }

    /// Stop following a platform, if nobody is still interested.
    ///
    /// The count is re-checked here, under the lock `listen` takes, because a
    /// surface can start listening between the last one dropping and this
    /// acquiring the lock. Dropping a subscription somebody has just taken an
    /// interest in would leave them attached to a connection that is about to
    /// be aborted — the same race the surface registry documents, in a new
    /// place.
    async fn release(&self, platform_id: &str) {
        let mut running = self.running.lock().await;

        let Some((interest, _)) = running.get(platform_id) else {
            return;
        };

        // One reference is the map's own.
        if Arc::strong_count(interest) > 1 {
            return;
        }

        running.remove(platform_id);
        tracing::debug!(platform = platform_id, "stopped following a platform");
    }

    /// What every subscription is doing, for an operator.
    pub async fn health(&self) -> BTreeMap<String, StreamHealth> {
        let running = self.running.lock().await;
        let mut health = BTreeMap::new();

        for (platform_id, (interest, held)) in running.iter() {
            let seen = held.held.lock().await;
            health.insert(
                platform_id.clone(),
                StreamHealth {
                    declared: true,
                    connected: seen.connected,
                    last_heard: seen.last_heard,
                    last_ended: seen.last_ended.clone(),
                    endings: seen.endings,
                    // Less the map's own reference.
                    watchers: Arc::strong_count(interest).saturating_sub(1),
                },
            );
        }

        health
    }

    /// How many platforms are being followed. For the tests.
    pub async fn following(&self) -> usize {
        self.running.lock().await.len()
    }
}

/// Hold one platform's stream open, reconnecting when it ends.
async fn follow_forever(
    client: reqwest::Client,
    platform_id: String,
    url: String,
    headers: Vec<(String, String)>,
    events: broadcast::Sender<Changed>,
    connected: tokio::sync::watch::Sender<bool>,
    held: Arc<Mutex<Held>>,
) {
    // The subscriber speaks mpsc, because one connection has one reader. The
    // fan-out to several surfaces is broadcast, and this bridges them.
    let (tell, mut heard) = tokio::sync::mpsc::channel(BACKLOG);

    let relaying = tokio::spawn({
        let held = held.clone();
        let events = events.clone();
        async move {
            while let Some(changed) = heard.recv().await {
                held.lock().await.last_heard = Some(Utc::now());
                // Fails only when no surface is subscribed at this instant,
                // which is ordinary and costs nothing: the poll underneath is
                // what makes an event safe to miss.
                let _ = events.send(changed);
            }
        }
    });

    let mut watching_connection = connected.subscribe();
    let noting = tokio::spawn({
        let held = held.clone();
        async move {
            while watching_connection.changed().await.is_ok() {
                let up = *watching_connection.borrow_and_update();
                let mut held = held.lock().await;
                held.connected = up;
                if up {
                    // Connecting counts as having heard something, so an
                    // operator can tell a stream that has just come up from one
                    // that has been up for an hour and said nothing.
                    held.last_heard = Some(Utc::now());
                }
            }
        }
    });

    loop {
        let ended =
            events::follow(&client, &url, &headers, &tell, events::SILENCE, &connected).await;
        let _ = connected.send(false);

        {
            let mut held = held.lock().await;
            held.last_ended = Some(ended.to_string());
            held.endings += 1;
        }

        if matches!(ended, Ended::NobodyWatching) {
            relaying.abort();
            noting.abort();
            return;
        }

        // Deliberately at info rather than as an error. Every panel of this
        // platform is back on its declared cadence while this is true, so
        // nothing a viewer can see is wrong — this is the shell losing an
        // optimisation, not a fault.
        tracing::info!(
            platform = platform_id,
            "not receiving events, polling instead: {ended}"
        );
        tokio::time::sleep(events::RESUBSCRIBE_AFTER).await;
    }
}
