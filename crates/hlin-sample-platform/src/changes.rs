//! Data that actually changes, and telling anyone who asked.
//!
//! Every other panel this platform serves is a pure function of the clock, which
//! is right for a demo — the same shapes on every run, and tests that can assert
//! on values. It is also the one thing that cannot demonstrate an event stream,
//! because a value derived from `now()` has no moment at which it changes.
//!
//! So this module gives the platform one panel with a real write path: a number
//! that a background task advances, on an irregular but deterministic schedule,
//! and which tells its subscribers when it does. That is what a real platform
//! has and what an event stream is for.
//!
//! **The panel is deliberately one that changes rarely.** Push saves nothing for
//! data that genuinely moves eight times a second: the shell would fetch just as
//! often, and would be right to. What it is for is data that must be seen
//! promptly and changes seldom — where polling makes you choose between being
//! late and being wasteful, and this lets you decline the choice.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::broadcast;

/// The panels whose data this platform actually mutates, and can therefore
/// honestly report on.
pub const PUSHED_PANELS: [&str; 3] = [BATCHES, PIPELINE, ACTIVITY];

/// A count that only goes up.
pub const BATCHES: &str = "batches";

/// The health of each stage in the pipeline.
///
/// The better of the two for showing why push exists. A stage going down is a
/// discrete event that somebody needs to see *now* and that happens rarely —
/// exactly the case where polling makes you choose between being late and being
/// wasteful. Polled to feel immediate it costs two requests a second; reported,
/// it costs two a minute and arrives sooner.
pub const PIPELINE: &str = "pipeline";

/// What just happened to the stages, most recent first.
///
/// The same events the pipeline panel colours, kept as a list so a person can
/// ask a stage what it has been doing rather than only what it is. Reported on
/// for the same reason: it changes at discrete moments and wants seeing at once.
pub const ACTIVITY: &str = "stage-activity";

/// The stages, in order. The shape of the pipeline never changes; only how the
/// stages are.
pub const STAGES: [&str; 5] = ["ingest", "parse", "enrich", "index", "archive"];

/// How many events to keep. Ten per stage is what the panel shows; the buffer
/// holds enough that every stage has ten even when one is much busier.
const KEPT: usize = 200;

/// How often the driver considers changing something.
const TICK_MS: u64 = 250;

/// Roughly one change in this many ticks, so about one every four seconds.
const RARITY: u64 = 16;

/// How many subscribers can fall behind before the oldest notices are dropped.
///
/// Small on purpose. A missed notification costs one relaxed poll interval of
/// staleness and nothing else, so there is no reason to hold a long history —
/// and a subscriber that far behind is better served by the poll underneath.
const BACKLOG: usize = 64;

/// A platform's mutable state, and who to tell about it.
#[derive(Debug)]
pub struct Changes {
    /// How many batches this platform has completed.
    batches: AtomicU64,

    /// How each stage is, indexed by [`STAGES`].
    ///
    /// A plain lock rather than atomics: this is read whole and written one
    /// entry at a time, which is what a lock is for, and nothing here is hot
    /// enough to care.
    stages: std::sync::Mutex<Vec<Health>>,

    /// What has happened to the stages, most recent last.
    events: std::sync::Mutex<std::collections::VecDeque<StageEvent>>,

    /// The panel keys whose data has just changed.
    told: broadcast::Sender<String>,
}

/// One thing that happened to one stage.
#[derive(Debug, Clone)]
pub struct StageEvent {
    /// Which stage, by its name.
    pub stage: &'static str,
    /// How it was.
    pub from: Health,
    /// How it is now.
    pub to: Health,
    /// When, in epoch milliseconds — the same clock a series carries, so
    /// nothing downstream has to convert between two notions of time.
    pub at_millis: i64,
}

/// How one stage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Working.
    Healthy,
    /// Working badly.
    Degraded,
    /// Not working.
    Down,
}

impl Health {
    /// The word a panel's row carries, which the design pack colours by.
    pub fn word(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Down => "down",
        }
    }

    /// Where a stage goes next.
    ///
    /// Down recovers through degraded rather than straight to healthy, because
    /// that is what recovering looks like and a graph that flips between two
    /// colours reads as a fault in the dashboard rather than in the pipeline.
    fn next(self) -> Self {
        match self {
            Self::Healthy => Self::Degraded,
            Self::Degraded => Self::Down,
            Self::Down => Self::Degraded,
        }
    }
}

impl Changes {
    /// State with nothing having happened yet.
    pub fn new() -> Arc<Self> {
        let (told, _) = broadcast::channel(BACKLOG);
        Arc::new(Self {
            batches: AtomicU64::new(0),
            stages: std::sync::Mutex::new(vec![Health::Healthy; STAGES.len()]),
            events: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(KEPT)),
            told,
        })
    }

    /// The current value.
    pub fn batches(&self) -> u64 {
        self.batches.load(Ordering::Relaxed)
    }

    /// How every stage is, in [`STAGES`] order.
    pub fn stages(&self) -> Vec<Health> {
        self.stages
            .lock()
            .expect("the stage lock is not poisoned")
            .clone()
    }

    /// Listen for panels whose data has changed.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.told.subscribe()
    }

    /// Whether anybody is listening, for the tests.
    pub fn listeners(&self) -> usize {
        self.told.receiver_count()
    }

    /// Advance the value and say so.
    ///
    /// The order matters and is the whole discipline of a push-capable
    /// platform: change the data first, then announce it. Announcing first
    /// races a shell fast enough to refetch before the write lands, which is a
    /// notification that makes the shell fetch the old value and then not fetch
    /// again until the relaxed interval.
    pub fn completed_a_batch(&self) {
        self.batches.fetch_add(1, Ordering::Relaxed);
        // Fails only when nobody is subscribed, which is the ordinary case for
        // a platform nobody is watching and not worth a word.
        let _ = self.told.send(BATCHES.to_string());
    }

    /// One stage changed how it is.
    ///
    /// Same discipline as above and the same reason: write, then announce. A
    /// shell fast enough to refetch on notice must not read the old state and
    /// then wait out the relaxed interval before looking again.
    pub fn stage_changed(&self, which: usize) {
        let Some(name) = STAGES.get(which) else {
            return;
        };
        let (from, to) = {
            let mut stages = self.stages.lock().expect("the stage lock is not poisoned");
            let Some(stage) = stages.get_mut(which) else {
                return;
            };
            let from = *stage;
            *stage = stage.next();
            (from, *stage)
        };
        self.record(name, from, to);
    }

    /// Something that was unhealthy comes back.
    ///
    /// Chosen from the stages that are actually unhealthy rather than from all
    /// of them. Picking at random and giving up when it landed on a healthy one
    /// made recovery several times rarer than it looked, so the pipeline
    /// drifted until nearly everything was coloured — measured on the demo at
    /// four degraded out of five, which reads as a broken dashboard rather than
    /// as a pipeline with a problem in it.
    ///
    /// Selecting among however many are currently unhealthy means recovery
    /// keeps pace with disturbance on its own, rather than needing a rate tuned
    /// against it.
    pub fn something_recovered(&self, nth: usize) {
        {
            let mut stages = self.stages.lock().expect("the stage lock is not poisoned");
            let unhealthy: Vec<usize> = stages
                .iter()
                .enumerate()
                .filter(|(_, stage)| **stage != Health::Healthy)
                .map(|(index, _)| index)
                .collect();

            if unhealthy.is_empty() {
                // Nothing is wrong, which is a fine state for a pipeline to be
                // in and nothing to announce.
                return;
            }
            let which = unhealthy[nth % unhealthy.len()];
            let from = stages[which];
            stages[which] = Health::Healthy;
            drop(stages);

            if let Some(name) = STAGES.get(which) {
                self.record(name, from, Health::Healthy);
            }
        }
    }

    /// Write down what happened, then say so.
    ///
    /// One place, so the log and the state can never disagree about what
    /// changed — and so the announcement is made once, after both are written,
    /// rather than once per thing that changed.
    fn record(&self, stage: &'static str, from: Health, to: Health) {
        {
            let mut events = self.events.lock().expect("the event lock is not poisoned");
            if events.len() == KEPT {
                events.pop_front();
            }
            events.push_back(StageEvent {
                stage,
                from,
                to,
                at_millis: now_millis(),
            });
        }

        // Both panels read this change: the graph colours by the state and the
        // list shows the transition. One write, two things to refetch.
        let _ = self.told.send(PIPELINE.to_string());
        let _ = self.told.send(ACTIVITY.to_string());
    }

    /// The most recent events for one stage, newest first.
    pub fn events_for(&self, stage: &str, most: usize) -> Vec<StageEvent> {
        self.events
            .lock()
            .expect("the event lock is not poisoned")
            .iter()
            .rev()
            .filter(|event| event.stage == stage)
            .take(most)
            .cloned()
            .collect()
    }
}

/// Advance the platform's state forever, irregularly.
///
/// Irregular because a value that changes on a fixed beat is one a poll could
/// have been scheduled against, and this exists to demonstrate the case where it
/// could not. Deterministic because the rest of this platform is, and a demo
/// that shows something different every run teaches nobody anything: the same
/// tick produces the same decision on every process that ever runs this code.
pub fn drive(changes: Arc<Changes>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(TICK_MS));
        let mut tick: u64 = 0;

        loop {
            ticker.tick().await;
            tick += 1;
            if should_change(tick) {
                changes.completed_a_batch();
            }

            // A stage takes a turn for the worse now and then, and whatever has
            // been unhealthy for a while comes back. Both are announced, and
            // neither is on a beat: a pipeline that degraded on a metronome
            // would be a pipeline you could have polled against.
            if let Some(which) = stage_to_disturb(tick) {
                changes.stage_changed(which);
            }
            if let Some(nth) = stage_to_recover(tick) {
                changes.something_recovered(nth);
            }
        }
    });
}

/// Whether this tick is one that changes something.
///
/// A fixed integer hash rather than a random number, so the sequence is the same
/// for all time and a test can assert on it.
///
/// The mixing is not decoration. The first version of this multiplied the tick
/// by an odd constant and took it modulo sixteen, which fires on exactly every
/// sixteenth tick — multiplying by an odd number modulo a power of two leaves
/// the low bits' period untouched, so what read as a hash was a metronome. The
/// test caught it. This is splitmix64's finaliser, which exists to move high
/// bits down into low ones, and that is the whole reason it is here.
fn should_change(tick: u64) -> bool {
    let mut mixed = tick.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed ^= mixed >> 27;
    mixed.is_multiple_of(RARITY)
}

/// Epoch milliseconds.
///
/// Its own helper because this module deliberately has no date crate: it needs
/// one instant, and the panels convert it.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// Which stage, if any, takes a turn for the worse on this tick.
fn stage_to_disturb(tick: u64) -> Option<usize> {
    pick(tick, 0x5bf0_3635, 10)
}

/// Which of the unhealthy stages, if any, recovers on this tick.
///
/// Slightly rarer than disturbance in raw rate, and still the thing that
/// keeps the pipeline mostly healthy — because recovery always lands on
/// something broken while disturbance often just deepens a problem that is
/// already there. That asymmetry settles at about one and a half stages
/// unhealthy: enough that a colour is nearly always showing somewhere, rare
/// enough that it means something, and occasionally one goes all the way down.
fn stage_to_recover(tick: u64) -> Option<usize> {
    pick(tick, 0x2f1e_9d47, 14)
}

/// One of the stages, one tick in `rarity`, mixed so the choice is irregular.
fn pick(tick: u64, salt: u64, rarity: u64) -> Option<usize> {
    let mut mixed = (tick ^ salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed ^= mixed >> 27;

    mixed
        .is_multiple_of(rarity)
        .then(|| (mixed >> 33) as usize % STAGES.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_irregular_but_not_random() {
        let first: Vec<u64> = (1..200).filter(|tick| should_change(*tick)).collect();
        let again: Vec<u64> = (1..200).filter(|tick| should_change(*tick)).collect();
        assert_eq!(first, again, "the same tick always decides the same way");

        assert!(!first.is_empty(), "something has to change");

        // Irregular: the gaps between changes are not all the same, which is
        // the property that makes this worth pushing rather than polling.
        let gaps: Vec<u64> = first.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            gaps.iter().any(|gap| *gap != gaps[0]),
            "a fixed beat could have been polled against: {gaps:?}"
        );
    }

    #[tokio::test]
    async fn advancing_changes_the_value_before_it_announces_it() {
        let changes = Changes::new();
        let mut listening = changes.subscribe();

        changes.completed_a_batch();

        let panel = listening.try_recv().expect("the listener was told");
        assert_eq!(panel, BATCHES);
        assert_eq!(
            changes.batches(),
            1,
            "a listener that refetches the instant it is told must not read the old value"
        );
    }

    #[tokio::test]
    async fn many_listeners_all_hear_about_it() {
        let changes = Changes::new();
        let mut listeners: Vec<_> = (0..8).map(|_| changes.subscribe()).collect();
        assert_eq!(changes.listeners(), 8);

        changes.completed_a_batch();

        for listening in &mut listeners {
            assert_eq!(
                listening.try_recv().expect("every listener was told"),
                BATCHES
            );
        }
    }

    #[tokio::test]
    async fn nobody_listening_is_not_a_failure() {
        // The ordinary case for a platform no shell is watching.
        let changes = Changes::new();
        changes.completed_a_batch();
        assert_eq!(changes.batches(), 1);
    }
}
