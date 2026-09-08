//! What the browser believes about a surface.
//!
//! Deliberately free of the DOM and of Leptos. Applying a frame, dropping a
//! superseded one, and deciding when a lost stream becomes an unreachable
//! shell are the parts worth being sure about, and they are all ordinary
//! functions here, tested on the host.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use hlin_manifest::Envelope;
use hlin_stream::{Frame, PanelFrame};
use hlin_view::{Cause, PanelState};

/// One panel, as the browser currently knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelView {
    /// Its id on this surface.
    pub instance: String,
    /// Where it is.
    pub state: PanelState,
    /// What it last received, where the state still shows it.
    pub envelope: Option<Envelope>,
    /// How old that is.
    pub age_seconds: Option<i64>,
    /// One line for the viewer, written by the shell.
    pub detail: Option<String>,
    /// What replaces it, where its platform said.
    pub successor: Option<String>,
    /// The kind the viewer chose, where it differs from the manifest default.
    pub kind_override: Option<String>,
    /// The title the viewer chose.
    pub title_override: Option<String>,
}

impl PanelView {
    fn from_frame(frame: &PanelFrame) -> Self {
        Self {
            instance: frame.instance.clone(),
            state: frame.panel_state(),
            envelope: frame.envelope.clone(),
            age_seconds: frame.age_seconds,
            detail: frame.detail.clone(),
            successor: frame.successor.clone(),
            kind_override: None,
            title_override: None,
        }
    }
}

/// The browser's view of one surface.
#[derive(Debug, Clone, Default)]
pub struct SurfaceState {
    panels: BTreeMap<String, PanelView>,
    generation: u64,
    /// The newest generation the shell has said it received.
    acknowledged: u64,
    /// When the stream was lost, if it is currently lost.
    lost_at: Option<DateTime<Utc>>,
}

impl SurfaceState {
    /// An empty surface.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every panel, in a stable order.
    pub fn panels(&self) -> Vec<&PanelView> {
        self.panels.values().collect()
    }

    /// One panel.
    pub fn panel(&self, instance: &str) -> Option<&PanelView> {
        self.panels.get(instance)
    }

    /// The generation the browser believes is in force.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the stream is currently lost.
    pub fn is_disconnected(&self) -> bool {
        self.lost_at.is_some()
    }

    /// Whether the stream has been gone long enough to stop expecting anything.
    ///
    /// The panels this state knows about are handled by [`Self::on_tick`]. This
    /// is for the ones it does not: a panel on the layout that has never had a
    /// frame, because the stream never opened. Nothing in here can say what
    /// that panel is doing — it has never been mentioned — so the surface has
    /// to ask, and the honest answer past the grace interval is that Hlin is
    /// not answering rather than that the panel is still loading.
    pub fn given_up(&self, now: DateTime<Utc>, grace: Duration) -> bool {
        self.lost_at.is_some_and(|lost_at| now - lost_at >= grace)
    }

    /// Whether the shell has taken the parameters the viewer last set.
    ///
    /// The browser moves its own generation forward the instant a control
    /// moves, and the shell acknowledges on its own schedule, so the gap
    /// between the two is exactly the interval where a person is looking at an
    /// answer to their previous question. Saying so beats leaving them to
    /// wonder whether the picker did anything.
    pub fn applied(&self) -> bool {
        self.acknowledged >= self.generation
    }

    /// Take a frame.
    ///
    /// A frame from a generation the browser has moved past is dropped here as
    /// well as at the shell. Two guards rather than one because the answer to
    /// an old time range arriving after a new one is the failure this exists to
    /// prevent, and it is cheap to be sure on both sides.
    pub fn apply(&mut self, frame: &Frame) -> bool {
        match frame {
            Frame::Surface(surface) => {
                if surface.generation < self.generation {
                    return false;
                }
                if surface.acknowledged {
                    self.acknowledged = self.acknowledged.max(surface.generation);
                }
                self.generation = surface.generation;
                true
            }

            Frame::Panel(panel) => {
                if panel.generation < self.generation {
                    return false;
                }
                self.generation = self.generation.max(panel.generation);

                let view = PanelView::from_frame(panel);
                match self.panels.get_mut(&panel.instance) {
                    Some(existing) => {
                        // The viewer's own choices are theirs, not the shell's,
                        // so a frame never overwrites them.
                        let kind = existing.kind_override.clone();
                        let title = existing.title_override.clone();
                        *existing = view;
                        existing.kind_override = kind;
                        existing.title_override = title;
                    }
                    None => {
                        self.panels.insert(panel.instance.clone(), view);
                    }
                }
                true
            }
        }
    }

    /// The browser is about to ask for new parameters.
    ///
    /// Returns the generation to send. Incrementing locally means a frame
    /// answering the previous one is recognisable as stale immediately, before
    /// the shell has acknowledged anything.
    pub fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// The stream went away.
    ///
    /// Everything becomes stale at once. This is the only state the browser
    /// derives for itself, and it exists because the shell cannot report its
    /// own absence ([[HLIN-A-0001]], as amended).
    pub fn on_stream_lost(&mut self, now: DateTime<Utc>) {
        if self.lost_at.is_some() {
            return;
        }
        self.lost_at = Some(now);

        for panel in self.panels.values_mut() {
            if matches!(panel.state, PanelState::Ready) {
                panel.state = PanelState::Stale;
            }
        }
    }

    /// Time passed while the stream was gone.
    ///
    /// Past the grace interval the panels are not merely aged: nothing is
    /// coming, and saying so is more honest than an indefinite "stale".
    pub fn on_tick(&mut self, now: DateTime<Utc>, grace: Duration) {
        let Some(lost_at) = self.lost_at else { return };
        if now - lost_at < grace {
            return;
        }

        for panel in self.panels.values_mut() {
            if !matches!(panel.state, PanelState::Unavailable(_)) {
                panel.state = PanelState::Unavailable(Cause::Unreachable);
                // The unreachable party is the shell, not any platform, and the
                // viewer should not be told to go and look at a platform.
                panel.detail = Some("Hlin is not responding".to_string());
            }
        }
    }

    /// The stream came back.
    ///
    /// Nothing is repaired here: the shell sends the current state of every
    /// panel on subscribe, so the browser becomes correct by being told rather
    /// than by guessing.
    pub fn on_stream_restored(&mut self) {
        self.lost_at = None;
    }

    /// The viewer chose a different rendering for a panel.
    pub fn set_kind(&mut self, instance: &str, kind: Option<String>) {
        if let Some(panel) = self.panels.get_mut(instance) {
            panel.kind_override = kind;
        }
    }
}
