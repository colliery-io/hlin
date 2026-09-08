//! The aggregator, connected to the network.
//!
//! Everything interesting is in [`super::aggregator`]; this drives it. It
//! fetches what the core says is due, hands back what came, and forwards
//! frames to whoever is subscribed.

use std::sync::{Arc, Weak};
use std::time::Duration as StdDuration;

use chrono::Utc;
use hlin_manifest::parse_envelope;
use tokio::sync::{Mutex, broadcast};

use super::aggregator::{Outcome, Request, Surface};
use crate::identity::Viewer;
use crate::registry::Registry;
use hlin_stream::Frame;

/// How long a surface keeps running after its last viewer leaves.
///
/// Not zero, because a browser that reconnects — an `EventSource` recovering
/// from a dropped connection, a laptop waking — should find its surface still
/// there with its data rather than rebuilding from nothing. Not long, because
/// every second of it is a surface fetching for nobody.
const IDLE_GRACE: StdDuration = StdDuration::from_secs(30);

/// One surface being served to one viewer.
pub struct LiveSurface {
    surface: Mutex<Surface>,
    frames: broadcast::Sender<Frame>,
    registry: Arc<Registry>,
    viewer: Viewer,
    client: reqwest::Client,
    /// The client for connections meant to stay open.
    ///
    /// Separate from the one above because that one carries the upstream
    /// timeout, which applies to the whole exchange including the body — so a
    /// held-open response is killed the moment it outlives it. Every
    /// subscription died after ten seconds and reconnected forever before this
    /// was separated, and the saving the feature exists for never appeared.
    stream_client: reqwest::Client,

    /// Every event stream the shell holds, shared across surfaces.
    streams: Arc<super::streams::Streams>,

    /// How often to look for work.
    ///
    /// Derived from the refresh interval by [`crate::config::Timings::tick`]
    /// rather than fixed, because a fixed tick is a floor on how live a panel
    /// can be: the driver cannot notice a refresh is due sooner than it looks.
    tick: StdDuration,
}

impl LiveSurface {
    /// Start serving a surface.
    pub fn new(
        surface: Surface,
        registry: Arc<Registry>,
        viewer: Viewer,
        client: reqwest::Client,
        stream_client: reqwest::Client,
        streams: Arc<super::streams::Streams>,
        tick: StdDuration,
    ) -> Arc<Self> {
        // Sized from the cadence rather than fixed. A constant 256 is about
        // three seconds of headroom for ten panels at 8Hz and over an hour at
        // the default half-minute — so the same number meant two entirely
        // different things, and on a fast shell a backgrounded tab could fall
        // behind it in the time it takes to switch windows.
        //
        // Bounded above because this is memory per surface per viewer, and a
        // browser that has fallen a full minute behind is better served by
        // being resent the current state than by a longer replay it will
        // mostly discard.
        let per_second = (1000 / tick.as_millis().max(1)) as usize;
        let buffered = (per_second * 30).clamp(256, 4096);
        let (frames, _) = broadcast::channel(buffered);
        Arc::new(Self {
            surface: Mutex::new(surface),
            frames,
            registry,
            viewer,
            client,
            stream_client,
            streams,
            tick,
        })
    }

    /// Subscribe to this surface's frames.
    pub fn subscribe(&self) -> broadcast::Receiver<Frame> {
        self.frames.subscribe()
    }

    /// How many browsers are attached.
    ///
    /// The broadcast sender already counts them, which is what makes stopping
    /// an unwatched surface cheap: no separate bookkeeping to get wrong.
    pub fn viewers(&self) -> usize {
        self.frames.receiver_count()
    }

    /// The state of everything, for a browser that has just arrived.
    pub async fn current(&self) -> Vec<Frame> {
        self.surface.lock().await.current_frames(Utc::now())
    }

    /// The layout was written; take what it now says.
    ///
    /// Selections are restored the same way a fresh surface restores them —
    /// without moving the generation or starting the settle timer, because
    /// these are not a change anybody just made, they are what the layout
    /// already said.
    pub async fn replace_instances(
        &self,
        instances: Vec<super::aggregator::Instance>,
        selections: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, Vec<String>>,
        >,
    ) {
        let frames = {
            let mut surface = self.surface.lock().await;
            surface.restore_selections(selections);
            surface.replace_instances(instances, Utc::now())
        };

        for frame in frames {
            let _ = self.frames.send(frame);
        }
    }

    /// A parameter change from the browser.
    pub async fn set_params(&self, request: hlin_stream::ParamsRequest) {
        let acknowledgement = {
            let mut surface = self.surface.lock().await;
            surface.set_params(
                request.generation,
                request.time_range,
                request.selections,
                Utc::now(),
            )
        };

        if let Some(frame) = acknowledgement {
            let _ = self.frames.send(frame);
        }
    }

    /// Take an interest in every platform on this surface that reports.
    ///
    /// The connections themselves belong to [`crate::stream::streams::Streams`],
    /// shared by every surface: this only says which platforms matter here and
    /// holds the handles that keep them open. A surface that ends drops them,
    /// and the last one to drop closes the connection.
    async fn follow_platforms(self: Arc<Self>) -> Vec<super::streams::Listening> {
        let wanted: std::collections::BTreeSet<String> = {
            let surface = self.surface.lock().await;
            surface
                .instances()
                .iter()
                .filter(|instance| instance.pushed && !instance.retired)
                .map(|instance| instance.platform_id.clone())
                .collect()
        };

        if wanted.is_empty() {
            return Vec::new();
        }

        let mut listening = Vec::new();
        let views = self.registry.views().await;

        for platform_id in wanted {
            let Some(view) = views.get(&platform_id) else {
                continue;
            };
            let Some(path) = view.manifest.as_ref().and_then(|m| m.events.clone()) else {
                // A panel that claims to be reported on, from a platform that
                // declares nowhere to hear it. Not an error — a platform
                // mid-adoption — and the panel is polled as it always was.
                continue;
            };

            // The shell subscribes as itself: the stream is per platform and
            // carries no data, so nothing in it could be one viewer's. A
            // strategy that can only speak for a person — `forward-session` —
            // fails here, and that platform is simply polled.
            let headers = match view.credentialer.headers(&super::events::shell_itself()) {
                Ok(headers) => headers,
                Err(reason) => {
                    tracing::info!(
                        platform = platform_id,
                        %reason,
                        "cannot subscribe as the shell itself, polling instead"
                    );
                    continue;
                }
            };

            let url = format!(
                "{}/{}",
                view.config.base_url.trim_end_matches('/'),
                path.trim_start_matches('/')
            );

            listening.push(
                self.streams
                    .listen(&platform_id, &url, headers, self.stream_client.clone())
                    .await,
            );
        }

        listening
    }

    /// Run until nobody is watching.
    ///
    /// `home` and `key` are how this takes itself out of [`crate::surfaces::Surfaces`]
    /// when the last browser goes. Without them a surface ran until the process
    /// exited: a tab closed, a browser navigated away or a reconnect that got a
    /// new key all left a driver fetching every panel forever, and the demo was
    /// measured sending 84 and 151 requests a second to its two platforms with
    /// nobody watching at all.
    pub async fn run(self: Arc<Self>, home: Weak<crate::surfaces::Surfaces>, key: String) {
        let mut ticker = tokio::time::interval(self.tick);
        let mut since_tick = 0u32;

        // An interest in every platform on this surface that reports. Holding
        // these is what keeps those connections open; when this driver returns
        // they drop, and the last surface to drop one closes it. Nothing else
        // has to be remembered, which is what makes the leak that
        // [[HLIN-T-0027]] fixed hard to reintroduce here.
        let mut listening = self.clone().follow_platforms().await;

        // When the last viewer left, or `None` while somebody is watching.
        //
        // A surface begins with nobody attached — the stream handler subscribes
        // just after `for_surface` returns — so this starts set, and a surface
        // built by a parameter POST that is never subscribed to stops on its
        // own rather than running forever.
        let mut unwatched_since: Option<std::time::Instant> = Some(std::time::Instant::now());

        // Staleness and the registry are checked once a second however fast the
        // driver is running, so a live surface does not take the registry lock
        // eight times a second to learn nothing.
        let per_second = (1000 / self.tick.as_millis().max(1)) as u32;

        loop {
            ticker.tick().await;

            // Anything the platforms said since the last tick, and whether each
            // is still connected. Drained rather than awaited: this loop has a
            // beat of its own and an event only moves a moment, so there is
            // nothing to wait for.
            if !listening.is_empty() {
                let mut surface = self.surface.lock().await;
                for held in &mut listening {
                    // Which interval this platform's panels are polled at
                    // follows the socket rather than the manifest.
                    let up = *held.connected.borrow_and_update();
                    surface.streaming_from(&held.platform_id, up);

                    loop {
                        match held.events.try_recv() {
                            Ok(changed) => {
                                surface.changed(&held.platform_id, &changed, Utc::now());
                            }
                            // Behind by more than the buffer holds. Nothing to
                            // catch up on: the missed events said panels
                            // changed, and the ones still here say so too.
                            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                                tracing::debug!(
                                    missed,
                                    "a surface fell behind a platform's events"
                                );
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            let (requests, frames) = {
                let mut surface = self.surface.lock().await;
                surface.due(Utc::now())
            };

            for frame in frames {
                let _ = self.frames.send(frame);
            }

            for request in requests {
                let surface = self.clone();
                tokio::spawn(async move { surface.fetch(request).await });
            }

            // Staleness and the registry are checked once a second rather than
            // every tick; there is nothing to gain from noticing either faster,
            // and the registry check takes a lock the polling loop also wants.
            since_tick += 1;
            if since_tick >= per_second.max(1) {
                since_tick = 0;
                let frames = {
                    let mut surface = self.surface.lock().await;
                    surface.tick(Utc::now())
                };
                for frame in frames {
                    let _ = self.frames.send(frame);
                }

                for frame in self.reconcile().await {
                    let _ = self.frames.send(frame);
                }

                // Whether anyone is still watching, checked on the same beat.
                if self.viewers() > 0 {
                    unwatched_since = None;
                    continue;
                }

                let idle_for = unwatched_since
                    .get_or_insert_with(std::time::Instant::now)
                    .elapsed();
                if idle_for < IDLE_GRACE {
                    continue;
                }

                let Some(home) = home.upgrade() else {
                    // The shell is shutting down. Nothing to take ourselves out
                    // of, and no reason to keep fetching.
                    return;
                };

                if home.release(&key, &self).await {
                    tracing::debug!(
                        surface = key,
                        seconds = idle_for.as_secs(),
                        "stopped a surface nobody was watching"
                    );
                    return;
                }

                // Somebody subscribed between the check and the lock.
                unwatched_since = None;
            }
        }
    }

    /// Bring the surface into line with what the registry currently accepts.
    ///
    /// A platform can withdraw a panel while somebody is looking at it. The
    /// shell knows before the platform would tell it — the manifest is polled —
    /// so the panel says it is gone rather than waiting for a fetch to fail and
    /// reporting the wrong reason ([[HLIN-A-0001]]).
    ///
    /// The important restraint: this only concludes anything about a platform
    /// it has actually heard from. An unreachable platform has an empty panel
    /// list, and treating that as "it withdrew everything" would turn a network
    /// problem into a contract one, which is exactly the confusion the state
    /// machine exists to prevent.
    async fn reconcile(&self) -> Vec<Frame> {
        let views = self.registry.views().await;

        let judged: Vec<(String, bool)> = {
            let surface = self.surface.lock().await;
            surface
                .instances()
                .iter()
                .filter(|instance| !instance.retired)
                .filter_map(|instance| {
                    let view = views.get(&instance.platform_id)?;
                    if !view.reachable || view.manifest.is_none() {
                        return None;
                    }
                    let offered = view
                        .accepted_panels()
                        .iter()
                        .any(|panel| panel.key == instance.panel_key);
                    Some((instance.id.clone(), offered))
                })
                .collect()
        };

        let now = Utc::now();
        let mut frames = Vec::new();
        let mut surface = self.surface.lock().await;

        for (instance, offered) in judged {
            let frame = if offered {
                surface.clear_registry_state(&instance, now)
            } else {
                surface.set_registry_state(&instance, hlin_view::Cause::Unknown, now)
            };
            if let Some(frame) = frame {
                frames.push(frame);
            }
        }

        frames
    }

    /// Fetch one request and give the answer back to the core.
    async fn fetch(self: Arc<Self>, request: Request) {
        let outcome = self.ask(&request).await;

        let frames = {
            let mut surface = self.surface.lock().await;
            surface.resolve(&request, outcome, Utc::now())
        };

        for frame in frames {
            let _ = self.frames.send(frame);
        }
    }

    async fn ask(&self, request: &Request) -> Outcome {
        // The credential is whatever this platform's strategy produces. The
        // aggregator never learns which strategy that is (decision
        // HLIN-A-0008).
        let (base_url, headers, promised) = {
            let views = self.registry.views().await;
            let Some(view) = views.get(&request.platform_id) else {
                return Outcome::Unreachable;
            };

            let headers = match view.credentialer.headers(&self.viewer) {
                Ok(headers) => headers,
                Err(reason) => {
                    // The shell could not produce a credential, which is a
                    // configuration problem rather than the platform's fault.
                    tracing::warn!(
                        platform = request.platform_id,
                        %reason,
                        "could not build a credential for this platform"
                    );
                    return Outcome::Malformed;
                }
            };

            let promised = view
                .panel_by_endpoint(&request.endpoint)
                .map(|panel| panel.envelope.clone());

            (view.config.base_url.clone(), headers, promised)
        };

        let url = if request.query.is_empty() {
            format!("{}/{}", base_url.trim_end_matches('/'), request.endpoint)
        } else {
            format!(
                "{}/{}?{}",
                base_url.trim_end_matches('/'),
                request.endpoint,
                request.query
            )
        };

        let mut outgoing = self.client.get(&url);
        for (name, value) in headers {
            outgoing = outgoing.header(name, value);
        }

        let response = match outgoing.send().await {
            Ok(response) => response,
            Err(_) => return Outcome::Unreachable,
        };

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Outcome::from_failure(status);
        }

        // Read against the same limit the envelope rules impose, rather than
        // buffering whatever arrives and measuring it afterwards. A platform
        // having a bad day costs one refused body, not the shell's memory.
        let body = match crate::bounded::read_bounded(
            response,
            hlin_manifest::envelope::MAX_DOCUMENT_BYTES,
        )
        .await
        {
            Ok(body) => body,
            Err(crate::bounded::TooMuch::Oversized { bytes, limit }) => {
                tracing::warn!(
                    platform = request.platform_id,
                    endpoint = request.endpoint,
                    bytes,
                    limit,
                    "platform sent more than an envelope may carry"
                );
                // Malformed rather than unreachable: the platform answered, and
                // what it answered is the problem — which is what an operator
                // needs to be told.
                return Outcome::Malformed;
            }
            Err(crate::bounded::TooMuch::Interrupted) => return Outcome::Unreachable,
        };

        // What the panel promised is what the shell insists on. A platform
        // that changed its shape without changing its manifest is caught here.
        //
        // A panel that has left the registry between the request and its answer
        // is `malformed` at this layer, not `unknown`: `unknown` is the
        // registry's word and it sets it without a fetch.
        let Some(promised) = promised.as_deref() else {
            // Silent until now, which made this the one panel state nobody
            // could diagnose: an operator saw "malformed" and went looking at
            // a platform that had answered perfectly well.
            tracing::warn!(
                platform = request.platform_id,
                endpoint = request.endpoint,
                "no accepted panel declares this endpoint any more, so there is \
                 nothing to check the answer against"
            );
            return Outcome::Malformed;
        };

        match parse_envelope(&body, promised) {
            Ok(envelope) => Outcome::Ready(Box::new(envelope)),
            Err(defect) => {
                tracing::warn!(
                    platform = request.platform_id,
                    endpoint = request.endpoint,
                    %defect,
                    "platform returned something its manifest did not promise"
                );
                Outcome::Malformed
            }
        }
    }
}
