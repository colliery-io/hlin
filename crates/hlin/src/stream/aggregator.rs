//! Deciding what every panel on a surface is doing.
//!
//! The aggregator owns the panel state machine (decision HLIN-A-0001), and
//! this module is that machine with the network taken out of it. It receives
//! events, updates state, and emits frames; something else does the fetching
//! and the waiting.
//!
//! Separating them is what makes the outcome table in HLIN-S-0003 testable at
//! all. Every row of it, the retry backoff, the coalescing and the generation
//! handling are exercised here against a clock the test controls, with no
//! sockets and no sleeping.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use hlin_manifest::Envelope;
use hlin_view::{Cause, PanelState};

use hlin_stream::{Frame, PanelFrame, SurfaceFrame, TimeRange};

/// How long data stays fresh, and how hard the shell tries.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// How often a subscribed panel is refetched, where it declares no cadence
    /// of its own.
    pub refresh: Duration,

    /// The fastest any panel may be refetched, however often its platform asks.
    ///
    /// A panel declaring `refresh_ms = 1` gets this rather than a thousand
    /// requests a second (decision HLIN-A-0009). The shell pays for the load,
    /// so the shell sets the floor.
    pub refresh_floor: Duration,
    /// When data becomes visibly aged.
    pub staleness: Duration,
    /// How long a parameter change waits before fanning out, so dragging a
    /// picker does not fetch on every move.
    pub settle: Duration,
    /// How long an event-prompted fetch is held before it is issued.
    ///
    /// One event for a panel twenty principals are watching with different
    /// selections is twenty fetches that the tick would have spread across an
    /// interval and an event would otherwise fire together. Instances are
    /// spread across this window, and repeated events for one instance inside
    /// it collapse into a single fetch.
    pub coalesce: Duration,

    /// The first wait after a platform fails.
    pub retry_from: Duration,
    /// The longest wait between retries.
    pub retry_ceiling: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            refresh: Duration::seconds(30),
            refresh_floor: Duration::milliseconds(100),
            coalesce: Duration::milliseconds(250),
            staleness: Duration::seconds(90),
            settle: Duration::milliseconds(250),
            retry_from: Duration::seconds(30),
            retry_ceiling: Duration::seconds(300),
        }
    }
}

/// What came back from a platform, already classified by the caller.
///
/// The mapping from HTTP outcome to this is in [`Outcome::from_status`], kept
/// here so the specification's table lives in one place.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A document that parsed and matched what the panel promised.
    Ready(Box<Envelope>),
    /// The platform did not answer usefully.
    Unreachable,
    /// It answered with something unusable.
    Malformed,
    /// It refused this viewer.
    Forbidden,
}

impl Outcome {
    /// What a status that is not a success means, per HLIN-S-0003.
    ///
    /// A 2xx is deliberately not here: it needs the parsed body, and whether
    /// the body is usable is the caller's to determine. Everything else can be
    /// decided from the status alone.
    ///
    /// The two rows that are easy to get wrong. A 403 is `forbidden`, which the
    /// viewer needs to see and act on; a 401 is `malformed`, because it means
    /// the shell's own credential was refused, which is an operator's problem
    /// and not the viewer's. And a 404 is `malformed` rather than `unknown`:
    /// the manifest says the panel exists, so the platform and its own
    /// manifest disagree, which is a defect rather than a removal.
    pub fn from_failure(status: u16) -> Self {
        match status {
            403 => Self::Forbidden,
            400..=499 => Self::Malformed,
            _ => Self::Unreachable,
        }
    }
}

/// An instant, as a platform is given it.
///
/// Absolute and in UTC, with `Z` rather than an offset: the shell resolves
/// anything relative before encoding, so a platform never sees `now-1h`
/// ([[HLIN-S-0002]] REQ-2.2).
fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One piece of a query string, escaped.
///
/// This is not decoration. An RFC 3339 timestamp with a `+00:00` offset carries
/// a `+`, which in a query string means a space, so an unescaped time range
/// arrives at a platform as an unparseable date and comes back a 400 — which
/// the shell then reports as `malformed`, blaming a platform that did nothing
/// wrong. The same hazard applies to any selection value a viewer can set,
/// which may contain `&`, `=` or a space.
///
/// Written out rather than pulled in, because the rule is short, the set of
/// unreserved characters is fixed by RFC 3986, and a dependency for eleven
/// lines is a dependency to keep updated forever.
fn encode(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                escaped.push(byte as char)
            }
            _ => escaped.push_str(&format!("%{byte:02X}")),
        }
    }
    escaped
}

/// Query keys the shell sets from the surface, which a selection may never
/// carry.
///
/// A platform declaring a control with one of these ids would collide with the
/// time range the picker drives, so the panel simply never receives it. Kept as
/// a belt beside `Instance::accepts`: a platform cannot grant a viewer the
/// ability to move everyone else's window by naming a parameter carelessly.
const RESERVED: [&str; 3] = ["from", "to", "step"];

/// The most values one parameter may carry upstream.
const MAX_VALUES: usize = 32;

/// The longest a single selection value may be.
///
/// Not a guess about platforms: an unbounded selections map becomes a request
/// line long enough for a platform to refuse, and the shell reports that
/// refusal as `malformed`, blaming a platform that did nothing wrong.
const MAX_VALUE_BYTES: usize = 256;

/// One panel on a surface, and everything the shell knows about it.
#[derive(Debug, Clone)]
pub struct Instance {
    /// Its id within the surface.
    pub id: String,
    /// The platform it comes from.
    pub platform_id: String,
    /// Its key in that platform's manifest.
    pub panel_key: String,
    /// The endpoint its data comes from, already resolved.
    pub endpoint: String,
    /// The envelope its manifest promised.
    pub envelope: String,
    /// What replaces it, where its platform said.
    pub successor: Option<String>,
    /// The parameter keys this panel said it responds to.
    ///
    /// A viewer's selections are filtered against this before they become query
    /// parameters. Without it, anything a browser put in a `selections` map
    /// reached the platform on the shell's own credential: a viewer could send
    /// `?admin=true`, or override the `from`/`to`/`step` the shell sets itself.
    /// The manifest already declares what a panel accepts and validation
    /// already checks those declarations; this is what makes the shell act on
    /// them.
    pub accepts: BTreeSet<String>,

    /// How often this panel's platform says its data is worth refetching.
    ///
    /// A hint, clamped by the surface's policy (decision HLIN-A-0009). The
    /// shell pays for the requests and answers for the load on every platform
    /// it fronts, so a panel asking to be polled faster than the shell allows
    /// gets the shell's floor — while one asking to be polled *less* often is
    /// always obeyed, since that costs nobody anything.
    pub refresh: Option<Duration>,

    /// Whether this panel's platform says it reports when this data changes.
    ///
    /// Half of what decides how often it is polled. The other half is whether
    /// that platform's stream is connected *right now* — a panel offered by a
    /// platform whose stream is down must go back to its declared cadence, or
    /// the shell would poll less on a promise nobody is currently keeping.
    pub pushed: bool,

    /// Whether the registry no longer offers this panel.
    ///
    /// A layout outlives the panels on it, so a surface can name one its
    /// platform has stopped declaring. Such an instance still exists and still
    /// draws — as `unavailable(unknown)` — but has no endpoint to ask, and
    /// asking anyway would turn a clear "no longer offered" into whatever the
    /// platform happens to answer at a URL it never promised.
    pub retired: bool,

    /// When an event said this instance should be refetched.
    ///
    /// A moment rather than a flag, because the answer to "one event, twenty
    /// watchers" is to spread them rather than to fire them together — and
    /// because a second event arriving before the first has been acted on has
    /// nothing to add, so it finds this already set and changes nothing.
    nudged: Option<DateTime<Utc>>,

    state: PanelState,
    envelope_held: Option<Envelope>,
    as_of: Option<DateTime<Utc>>,
    fetched_at: Option<DateTime<Utc>>,
    generation: u64,
}

impl Instance {
    /// A panel instance with nothing fetched yet.
    pub fn new(
        id: impl Into<String>,
        platform_id: impl Into<String>,
        panel_key: impl Into<String>,
        endpoint: impl Into<String>,
        envelope: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            platform_id: platform_id.into(),
            panel_key: panel_key.into(),
            endpoint: endpoint.into(),
            envelope: envelope.into(),
            successor: None,
            accepts: BTreeSet::new(),
            refresh: None,
            pushed: false,
            retired: false,
            nudged: None,
            state: PanelState::Loading,
            envelope_held: None,
            as_of: None,
            fetched_at: None,
            generation: 0,
        }
    }

    /// A panel a layout names that its platform no longer offers.
    ///
    /// Born unavailable and never fetched. It exists so the surface can say
    /// what happened to a panel someone put there, rather than quietly drawing
    /// one fewer than the layout says.
    pub fn retired(
        id: impl Into<String>,
        platform_id: impl Into<String>,
        panel_key: impl Into<String>,
    ) -> Self {
        let mut instance = Self::new(id, platform_id, panel_key, String::new(), String::new());
        instance.retired = true;
        instance.state = PanelState::Unavailable(Cause::Unknown);
        instance
    }

    /// Where this panel is now.
    pub fn state(&self) -> PanelState {
        self.state
    }

    /// The last document it received, if it still holds one.
    pub fn envelope(&self) -> Option<&Envelope> {
        self.envelope_held.as_ref()
    }
}

/// What the shell should fetch next.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Request {
    /// The platform to ask.
    pub platform_id: String,
    /// The endpoint.
    pub endpoint: String,
    /// The query, already canonical, so two instances asking for the same
    /// thing produce the same key.
    pub query: String,
    /// Which generation this answers.
    pub generation: u64,
    /// Every instance waiting on it. One request, many frames.
    pub instances: Vec<String>,
}

impl Request {
    /// What makes two requests the same request.
    ///
    /// The principal is not here because a surface belongs to one principal;
    /// the aggregator holds one surface, so every request it makes is already
    /// for the same person. Deduplication *across* principals would be a
    /// correctness bug (decision HLIN-A-0004), and the shape of this key is
    /// what prevents it.
    pub fn key(&self) -> (String, String, String) {
        (
            self.platform_id.clone(),
            self.endpoint.clone(),
            self.query.clone(),
        )
    }
}

/// One surface, and every panel on it.
pub struct Surface {
    instances: Vec<Instance>,
    generation: u64,
    time_range: Option<TimeRange>,
    selections: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    policy: Policy,

    /// When a pending parameter change should be acted on.
    settling_until: Option<DateTime<Utc>>,
    /// How long to wait before asking a failing platform again.
    backoff: BTreeMap<String, Duration>,
    /// When each platform may next be asked.
    next_attempt: BTreeMap<String, DateTime<Utc>>,

    /// The platforms whose event stream is connected right now.
    ///
    /// Not "declares one" — connected. A panel is polled at the relaxed
    /// interval only while something is actually reporting on it, so a stream
    /// that drops takes every panel of that platform straight back to its
    /// declared cadence, with no recovery step, because the poll was never
    /// turned off (HLIN-S-0006 REQ-1.4).
    streaming: std::collections::BTreeSet<String>,
}

impl Surface {
    /// A surface holding these panels.
    pub fn new(instances: Vec<Instance>, policy: Policy) -> Self {
        Self {
            instances,
            generation: 1,
            time_range: None,
            selections: BTreeMap::new(),
            policy,
            settling_until: None,
            streaming: std::collections::BTreeSet::new(),
            backoff: BTreeMap::new(),
            next_attempt: BTreeMap::new(),
        }
    }

    /// The choices this surface's layout was stored with.
    ///
    /// Applied before anything is fetched, and deliberately without moving the
    /// generation or starting the settle timer: these are not a change anyone
    /// just made, they are what the layout already said. Without this, a person
    /// who chose a cluster and reloaded would get the platform's default until
    /// they touched a control, and would have no way to know why.
    pub fn restore_selections(
        &mut self,
        selections: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    ) {
        self.selections = selections;
    }

    /// Take the panels this surface should now be showing.
    ///
    /// A layout write used to drop the whole surface, so the next subscription
    /// rebuilt it and every panel refetched from nothing — a visible blink on
    /// every drag, for a move that changed no data at all.
    ///
    /// What survives is decided per panel and deliberately narrowly: an
    /// instance keeps what it fetched only when its id, endpoint and envelope
    /// are all unchanged. A panel that moved is the same panel. One whose
    /// platform now serves it from a different endpoint, or promises a
    /// different envelope, is not — and showing what the old one held under the
    /// new one's name would be the shell asserting something nobody told it.
    ///
    /// Returns the frames to send, so a browser watching sees the new
    /// arrangement without asking for it.
    pub fn replace_instances(&mut self, next: Vec<Instance>, now: DateTime<Utc>) -> Vec<Frame> {
        let held: BTreeMap<String, Instance> = std::mem::take(&mut self.instances)
            .into_iter()
            .map(|instance| (instance.id.clone(), instance))
            .collect();

        self.instances = next
            .into_iter()
            .map(|mut instance| {
                let Some(previous) = held.get(&instance.id) else {
                    return instance;
                };

                if previous.endpoint != instance.endpoint
                    || previous.envelope != instance.envelope
                    || previous.retired != instance.retired
                {
                    return instance;
                }

                instance.state = previous.state;
                instance.envelope_held.clone_from(&previous.envelope_held);
                instance.as_of = previous.as_of;
                instance.fetched_at = previous.fetched_at;
                instance.generation = previous.generation;
                instance
            })
            .collect();

        // Every panel, not only the changed ones. A browser has just been told
        // the layout was written and has no way to know which panels it kept,
        // so the honest answer is the state of everything — which is what a
        // fresh subscription would have given it anyway, minus the refetch.
        self.current_frames(now)
    }

    /// The generation in force.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Every panel, for a caller that wants to inspect them.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// One panel by id.
    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|instance| instance.id == id)
    }

    /// The current state of the surface and every panel on it, as frames.
    ///
    /// Sent when a browser subscribes or reconnects, so it is correct without
    /// the shell replaying anything (HLIN-S-0003 NFR-1.1).
    ///
    /// The surface frame comes first and is part of "current state", not an
    /// extra. A browser that posted parameters and then subscribed would
    /// otherwise never hear the acknowledgement — it was broadcast to nobody —
    /// and would sit reporting the surface as pending forever. Panel frames
    /// alone do not close that, because a panel frame says what one panel is
    /// doing, not which question the surface is currently answering.
    pub fn current_frames(&self, now: DateTime<Utc>) -> Vec<Frame> {
        let mut frames = vec![Frame::Surface(SurfaceFrame::acknowledging(self.generation))];
        frames.extend(
            self.instances
                .iter()
                .map(|instance| self.frame_for(instance, now)),
        );
        frames
    }

    /// A parameter change arrived.
    ///
    /// A platform's event stream came up, or went away.
    ///
    /// The only thing that moves a panel between the two rows of the interval
    /// table. Deliberately a fact the driver reports rather than something the
    /// aggregator infers: whether a socket is open is not knowable from here,
    /// and guessing it is how a shell ends up polling less than it should.
    pub fn streaming_from(&mut self, platform_id: &str, connected: bool) {
        if connected {
            self.streaming.insert(platform_id.to_string());
        } else {
            self.streaming.remove(platform_id);
        }
    }

    /// A platform said one of its panels changed.
    ///
    /// Marks every matching instance to be fetched sooner than its cadence
    /// would have. Nothing is fetched here: this only moves a moment, and the
    /// tick that was always going to run does the rest — which is what keeps
    /// push from being a second code path with its own failure modes.
    ///
    /// Returns how many instances were nudged, so a caller can say nothing
    /// happened. An event about a panel nobody is watching returns zero and
    /// costs one comparison per instance (REQ-2.2).
    pub fn changed(
        &mut self,
        platform_id: &str,
        event: &super::events::Changed,
        now: DateTime<Utc>,
    ) -> usize {
        let window = self.policy.coalesce;
        let mut nudged = 0;

        for instance in &mut self.instances {
            if instance.platform_id != platform_id
                || instance.panel_key != event.panel
                || instance.retired
            {
                continue;
            }

            // Subset matching against what this instance actually selected, so
            // a platform can say "west changed" without enumerating every
            // combination of every other control a viewer might have set.
            let chosen = self
                .selections
                .get(&instance.id)
                .cloned()
                .unwrap_or_default();
            if !event.matches(&chosen) {
                continue;
            }

            // Already owed a fetch. A second event before the first has been
            // acted on has nothing to add — which is the coalescing, and it
            // falls out of the moment already being set rather than needing a
            // separate count.
            if instance.nudged.is_some() {
                continue;
            }

            instance.nudged = Some(now + spread_within(&instance.id, window));
            nudged += 1;
        }

        nudged
    }

    /// Returns the acknowledgement to send. The fan-out itself waits for the
    /// settle interval, so dragging a picker produces one round of requests
    /// rather than one per movement.
    pub fn set_params(
        &mut self,
        generation: u64,
        time_range: Option<TimeRange>,
        selections: BTreeMap<String, BTreeMap<String, Vec<String>>>,
        now: DateTime<Utc>,
    ) -> Option<Frame> {
        // An out-of-order message must not rewind a surface.
        if generation < self.generation {
            return None;
        }

        self.generation = generation;
        if let Some(range) = time_range {
            self.time_range = Some(range);
        }
        self.selections = selections;
        self.settling_until = Some(now + self.policy.settle);

        Some(Frame::Surface(SurfaceFrame::acknowledging(generation)))
    }

    /// What should be fetched now, and the frames that go with starting it.
    ///
    /// Returns nothing while a parameter change is still settling, which is
    /// the whole of the coalescing behaviour.
    pub fn due(&mut self, now: DateTime<Utc>) -> (Vec<Request>, Vec<Frame>) {
        if let Some(until) = self.settling_until {
            if now < until {
                return (Vec::new(), Vec::new());
            }
            self.settling_until = None;
        }

        let mut wanted: BTreeMap<(String, String, String), Request> = BTreeMap::new();
        let mut frames = Vec::new();

        for index in 0..self.instances.len() {
            let instance = &self.instances[index];

            // A panel its platform no longer offers has nothing to be asked.
            // That covers both a layout naming one that was never there on this
            // run, and one the registry has since withdrawn: asking either
            // would hit an endpoint the current manifest does not declare, and
            // the 404 would turn a clear "no longer offered" into "malformed"
            // on every refresh.
            if instance.retired || instance.state == PanelState::Unavailable(Cause::Unknown) {
                continue;
            }

            // A platform that has been failing is left alone until its backoff
            // elapses, and the wait is per platform rather than per panel so a
            // platform that is down is asked once per interval, not once per
            // panel per viewer.
            if let Some(next) = self.next_attempt.get(&instance.platform_id)
                && now < *next
            {
                continue;
            }

            // The panel's own cadence where it declared one, never faster than
            // the shell allows and never slower than the shell insists on
            // asking. A platform may always ask to be left alone for longer.
            let declared = match instance.refresh {
                Some(asked) => asked.max(self.policy.refresh_floor),
                None => self.policy.refresh,
            };

            // Unless this panel is being reported on *and* its platform's
            // stream is connected right now, in which case polling is a safety
            // net rather than a cadence (specification HLIN-S-0006).
            //
            // `max` of the two, and both halves matter. Never faster than the
            // panel asked for, because a panel declaring a slow cadence meant
            // it and being pushed is not a reason to poll it more. Never slower
            // than the shell's own default, because that number is already the
            // operator's answer to how stale a panel may get when nothing is
            // telling us otherwise — which is exactly the question here.
            let relaxed = instance.pushed && self.streaming.contains(&instance.platform_id);
            let refresh = if relaxed {
                declared.max(self.policy.refresh)
            } else {
                declared
            };

            // An event moves a fetch earlier, and the floor still applies: a
            // platform emitting a thousand events a second gets exactly what a
            // panel asking to be polled a thousand times a second gets, which
            // is the shell's own limit (REQ-2.4). The shell pays either way.
            let nudged = match (instance.nudged, instance.fetched_at) {
                (None, _) => false,
                (Some(at), None) => now >= at,
                (Some(at), Some(fetched)) => {
                    now >= at && now - fetched >= self.policy.refresh_floor
                }
            };

            let due = nudged
                || instance.generation != self.generation
                || match instance.fetched_at {
                    None => true,
                    Some(at) => now - at >= refresh,
                };
            if !due {
                continue;
            }

            let query = self.query_for(instance);
            let request = Request {
                platform_id: instance.platform_id.clone(),
                endpoint: instance.endpoint.clone(),
                query,
                generation: self.generation,
                instances: vec![instance.id.clone()],
            };

            // Two panels asking for the same thing become one request.
            wanted
                .entry(request.key())
                .and_modify(|existing| existing.instances.push(instance.id.clone()))
                .or_insert(request);

            // The nudge is spent. Anything arriving from here is a new claim
            // about new data, and holding the old one would fetch twice.
            self.instances[index].nudged = None;

            // A panel with nothing to show says so; one holding data keeps
            // showing it while the next fetch is in flight.
            if self.instances[index].envelope_held.is_none() {
                self.instances[index].state = PanelState::Loading;
                let instance = &self.instances[index];
                frames.push(self.frame_for(instance, now));
            }
        }

        (wanted.into_values().collect(), frames)
    }

    /// A request came back.
    ///
    /// One outcome, one frame per instance that was waiting on it.
    pub fn resolve(
        &mut self,
        request: &Request,
        outcome: Outcome,
        now: DateTime<Utc>,
    ) -> Vec<Frame> {
        // A frame from a superseded generation is never emitted: a slow answer
        // to an old time range must not paint over a newer one.
        if request.generation != self.generation {
            return Vec::new();
        }

        match &outcome {
            Outcome::Unreachable => {
                let wait = self
                    .backoff
                    .get(&request.platform_id)
                    .map(|current| {
                        Duration::seconds(
                            (current.num_seconds() * 2)
                                .min(self.policy.retry_ceiling.num_seconds()),
                        )
                    })
                    .unwrap_or(self.policy.retry_from);
                self.backoff.insert(request.platform_id.clone(), wait);
                self.next_attempt
                    .insert(request.platform_id.clone(), now + wait);
            }
            _ => {
                // Any answer at all, even a refusal, means the platform is
                // there; only silence earns a backoff.
                self.backoff.remove(&request.platform_id);
                self.next_attempt.remove(&request.platform_id);
            }
        }

        let mut frames = Vec::new();

        for id in &request.instances {
            let Some(index) = self.instances.iter().position(|held| &held.id == id) else {
                continue;
            };

            let instance = &mut self.instances[index];
            instance.generation = request.generation;
            instance.fetched_at = Some(now);

            match &outcome {
                Outcome::Ready(envelope) => {
                    instance.as_of = envelope.as_of().or(Some(now));
                    instance.envelope_held = Some((**envelope).clone());
                    instance.state = PanelState::Ready;
                }

                // Only unreachable keeps what it had. The others mean the data
                // is wrong or not this viewer's to see, and showing it would
                // mislead.
                Outcome::Unreachable => {
                    instance.state = PanelState::Unavailable(Cause::Unreachable);
                }
                Outcome::Malformed => {
                    instance.envelope_held = None;
                    instance.state = PanelState::Unavailable(Cause::Malformed);
                }
                Outcome::Forbidden => {
                    instance.envelope_held = None;
                    instance.state = PanelState::Unavailable(Cause::Forbidden);
                }
            }

            let instance = &self.instances[index];
            frames.push(self.frame_for(instance, now));
        }

        frames
    }

    /// The registry says a panel is gone, or on its way out.
    ///
    /// No fetch is involved: the shell already knows, so asking the platform
    /// would only confirm it slowly.
    pub fn set_registry_state(
        &mut self,
        instance_id: &str,
        cause: Cause,
        now: DateTime<Utc>,
    ) -> Option<Frame> {
        let index = self
            .instances
            .iter()
            .position(|instance| instance.id == instance_id)?;

        // Saying the same thing again is not news. The reconciliation pass runs
        // once a second, and a panel that has been withdrawn would otherwise
        // send a frame every second for as long as anyone watched it.
        if self.instances[index].state == PanelState::Unavailable(cause) {
            return None;
        }

        self.instances[index].state = PanelState::Unavailable(cause);
        // The data goes with the panel, and so does its age. A panel showing
        // nothing that still reports how old the nothing is would be reporting
        // the age of something a viewer can no longer see.
        self.instances[index].envelope_held = None;
        self.instances[index].as_of = None;
        self.instances[index].fetched_at = None;

        let instance = &self.instances[index];
        Some(self.frame_for(instance, now))
    }

    /// The registry offers a panel again, having stopped.
    ///
    /// Back to `loading`, so the next round fetches it. Nothing is assumed
    /// about what it will say: a panel that has been away is a panel with no
    /// current data, and pretending otherwise would show a viewer a number
    /// from before the platform changed its mind.
    ///
    /// Returns nothing when the panel was not in the state this undoes, so a
    /// reconciliation pass running every second does not reset a healthy panel
    /// once a second.
    pub fn clear_registry_state(&mut self, instance_id: &str, now: DateTime<Utc>) -> Option<Frame> {
        let index = self
            .instances
            .iter()
            .position(|instance| instance.id == instance_id)?;

        if self.instances[index].state != PanelState::Unavailable(Cause::Unknown) {
            return None;
        }

        self.instances[index].state = PanelState::Loading;
        let instance = &self.instances[index];
        Some(self.frame_for(instance, now))
    }

    /// Time passed: anything that has aged past the staleness window says so.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Vec<Frame> {
        let mut frames = Vec::new();

        for index in 0..self.instances.len() {
            let instance = &self.instances[index];
            if instance.state != PanelState::Ready {
                continue;
            }

            let reference = instance.as_of.or(instance.fetched_at);
            let Some(reference) = reference else { continue };

            if now - reference >= self.policy.staleness {
                self.instances[index].state = PanelState::Stale;
                let instance = &self.instances[index];
                frames.push(self.frame_for(instance, now));
            }
        }

        frames
    }

    /// The canonical query for one instance: the surface's time range, its own
    /// selections, sorted so two instances that wrote the same thing
    /// differently still share a request.
    fn query_for(&self, instance: &Instance) -> String {
        let mut parts: Vec<(String, String)> = Vec::new();

        if let Some(range) = self.time_range {
            // `Z` rather than `+00:00`. Both are RFC 3339 and both are encoded
            // correctly below, but a platform's logs and a person's `curl` are
            // easier to read without three percent escapes in every timestamp.
            parts.push(("from".to_string(), instant(range.from)));
            parts.push(("to".to_string(), instant(range.to)));
            parts.push(("step".to_string(), range.step_seconds(600).to_string()));
        }

        if let Some(chosen) = self.selections.get(&instance.id) {
            for (parameter, values) in chosen {
                // Only what the panel declared. A selections map arrives from a
                // browser and is stored verbatim with the layout, so without
                // this a viewer could put any key in it and have the shell send
                // it upstream under the shell's own credential — `?admin=true`
                // to a platform that reads its query string, or a `from` that
                // overrides the window the picker set. The manifest says what a
                // panel responds to; this is where the shell believes it.
                if RESERVED.contains(&parameter.as_str()) || !instance.accepts.contains(parameter) {
                    tracing::debug!(
                        instance = instance.id,
                        panel = instance.panel_key,
                        parameter,
                        "dropped a selection this panel does not declare"
                    );
                    continue;
                }

                for value in values.iter().take(MAX_VALUES) {
                    if value.len() > MAX_VALUE_BYTES {
                        tracing::debug!(
                            instance = instance.id,
                            parameter,
                            bytes = value.len(),
                            "dropped an oversized selection value"
                        );
                        continue;
                    }
                    parts.push((parameter.clone(), value.clone()));
                }
            }
        }

        // Sorted before encoding, so two instances that wrote the same thing in
        // a different order still produce one request and share a fetch.
        parts.sort();

        parts
            .iter()
            .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn frame_for(&self, instance: &Instance, now: DateTime<Utc>) -> Frame {
        let mut frame = PanelFrame::new(&instance.id, self.generation, instance.state);

        let shows_data = instance.state.shows_data();
        if shows_data {
            frame.envelope = instance.envelope_held.clone();
        }

        frame.as_of = instance.as_of;
        frame.age_seconds = instance
            .as_of
            .or(instance.fetched_at)
            .map(|at| (now - at).num_seconds().max(0));

        if let PanelState::Unavailable(cause) = instance.state {
            // Written by the shell from the cause, never passed through from a
            // platform: a platform's error body may contain anything, and the
            // shell does not put text a platform wrote in front of a person.
            frame.detail = Some(explain(cause).to_string());
            if cause.offers_successor() {
                frame.successor.clone_from(&instance.successor);
            }
        }

        Frame::Panel(Box::new(frame))
    }
}

/// One line for the viewer, from the closed set of causes.
fn explain(cause: Cause) -> &'static str {
    match cause {
        Cause::Unreachable => "this platform is not responding",
        Cause::Malformed => "this platform sent something the shell could not use",
        Cause::Unknown => "this panel is no longer offered",
        Cause::Deprecated => "this panel has been retired",
        Cause::Forbidden => "you do not have access to this panel",
    }
}

/// Where in the coalescing window this instance's fetch falls.
///
/// Derived from the instance's own id rather than drawn at random, which gets
/// the spread without the unpredictability: twenty instances of one panel land
/// at twenty different points in the window, and the same instance lands in the
/// same place every time, so a test can assert on it.
///
/// A random offset would make every test of this either flaky or forced to
/// inject a generator, and would buy nothing — the property wanted here is that
/// distinct instances differ, not that anybody cannot guess them.
fn spread_within(instance_id: &str, window: Duration) -> Duration {
    let millis = window.num_milliseconds().max(0);
    if millis == 0 {
        return Duration::zero();
    }

    // FNV-1a, for an answer that is stable across runs. The standard library's
    // hasher is explicitly allowed to differ between them.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in instance_id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    Duration::milliseconds((hash % millis as u64) as i64)
}
