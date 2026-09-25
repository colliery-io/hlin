//! The parent end of the module bridge, as decisions (specification
//! HLIN-S-0007, *Frames*, *Messages*, *Limits* and *Panel states*).
//!
//! Free of the DOM, like [`crate::state`] and [`crate::grid`], and for the same
//! reason: which frame a message is from, when a quiet module becomes a stale
//! one and then an absent one, when a module has asked for too much, and which
//! paths a module may send the page's session to are the rules that fail
//! silently when they are wrong. They are ordinary functions of time and
//! input here, tested on the host. [`crate::frame`] is the wiring that feeds
//! them events and does what they say.

use std::collections::{BTreeMap, VecDeque};

use hlin_bridge::{ALLOWED_RESPONSE_HEADERS, Refusal, SUPPORTED_BRIDGE_MAJORS};
use hlin_view::{Cause, PanelState};

/// How often `init` is resent while a module has not said `ready`.
///
/// A module's code starts after its frame's `load` event (a Trunk-built module
/// compiles its WebAssembly first), so an `init` sent once can arrive before
/// anything listens and be lost, which would look exactly like a module that
/// never answers. A module acts on the first and ignores the rest.
pub const INIT_RESEND_MS: f64 = 250.0;

/// How often a visible, ready module is asked whether it is there.
pub const HEARTBEAT_MS: f64 = hlin_bridge::HEARTBEAT_INTERVAL_MS as f64;

/// How long a frame has to say `ready` once it has loaded.
pub const READY_TIMEOUT_MS: f64 = hlin_bridge::READY_TIMEOUT_MS as f64;

/// Unanswered heartbeats in a row that make a module `unavailable`. One makes
/// it `stale`.
pub const MISSES_UNTIL_UNREACHABLE: u32 = 3;

/// How close to the viewport a panel comes before its frame is mounted, as the
/// `rootMargin` an observer takes. Close enough that a module has started by
/// the time a person scrolls to it, far enough that a long surface does not
/// load every module below the fold.
pub const MOUNT_MARGIN: &str = "200px";

// -- The registry ----------------------------------------------------------

/// What a frame is, as far as the page is concerned: whose it is, and which
/// panel it draws. Everything a message is attributed to comes from here, never
/// from the message.
#[derive(Debug, Clone, PartialEq)]
pub struct Registered {
    /// The platform whose module this is. The only platform its `fetch` can
    /// reach.
    pub platform: String,
    /// The panel key.
    pub panel: String,
    /// The panel instance on this surface.
    pub instance: String,
    /// When the frame was put in the document, in milliseconds.
    pub mounted_at: f64,
}

/// The frames the page created and has not torn down, by the window each one
/// holds.
///
/// Generic over the window handle so it can be tested without a browser; the
/// page uses the frame's `contentWindow`, which is what `event.source` names
/// and is the one thing about a message a module cannot forge (REQ-2.1).
#[derive(Debug, Clone)]
pub struct Registry<W> {
    frames: Vec<(W, Registered)>,
}

impl<W> Default for Registry<W> {
    fn default() -> Self {
        Self { frames: Vec::new() }
    }
}

impl<W: PartialEq> Registry<W> {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// A frame the page has just put in the document.
    ///
    /// An instance can only ever have one frame, so a second registration for
    /// the same instance replaces the first rather than leaving two windows
    /// able to speak for it.
    pub fn register(&mut self, window: W, frame: Registered) {
        self.frames
            .retain(|(_, existing)| existing.instance != frame.instance);
        self.frames.push((window, frame));
    }

    /// Who sent a message, if the page created the window it came from.
    ///
    /// `None` is the answer for everything else, and the message is dropped:
    /// another tab, a frame of a frame, a frame already torn down.
    pub fn source(&self, window: &W) -> Option<&Registered> {
        self.frames
            .iter()
            .find(|(held, _)| held == window)
            .map(|(_, frame)| frame)
    }

    /// Forget an instance's frame. Called before the frame leaves the
    /// document, so a message already in flight from it finds nobody.
    pub fn remove(&mut self, instance: &str) -> Option<Registered> {
        let index = self
            .frames
            .iter()
            .position(|(_, frame)| frame.instance == instance)?;
        Some(self.frames.remove(index).1)
    }

    /// The window registered for an instance.
    pub fn window(&self, instance: &str) -> Option<&W> {
        self.frames
            .iter()
            .find(|(_, frame)| frame.instance == instance)
            .map(|(window, _)| window)
    }

    /// How many frames are registered.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

// -- Liveness --------------------------------------------------------------

/// Something the page owes a module now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Send (or resend) `init`.
    Init,
    /// Send `heartbeat` with this count.
    Heartbeat(u64),
}

/// A module panel's state, derived from the bridge and nothing else (*Panel
/// states*). Never from what the module draws.
///
/// Time is milliseconds from any fixed origin, passed in, so every rule here
/// is checked without waiting for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Liveness {
    state: PanelState,
    mounted_at: f64,
    loaded_at: Option<f64>,
    last_init: Option<f64>,
    /// The last heartbeat count sent.
    beat: u64,
    /// The heartbeat still waiting for its echo.
    outstanding: Option<u64>,
    last_beat_at: f64,
    misses: u32,
    visible: bool,
}

impl Liveness {
    /// A frame just mounted: `loading`, and visible until told otherwise.
    pub fn new(now: f64) -> Self {
        Self {
            state: PanelState::Loading,
            mounted_at: now,
            loaded_at: None,
            last_init: None,
            beat: 0,
            outstanding: None,
            last_beat_at: now,
            misses: 0,
            visible: true,
        }
    }

    /// Where the panel is.
    pub fn state(&self) -> PanelState {
        self.state
    }

    /// Whether the frame's document has loaded.
    pub fn has_loaded(&self) -> bool {
        self.loaded_at.is_some()
    }

    /// The frame's document loaded. The `ready` timeout runs from here, and
    /// `init` is due at once.
    pub fn loaded(&mut self, now: f64) {
        if self.loaded_at.is_none() {
            self.loaded_at = Some(now);
        }
    }

    /// The module said `ready`, speaking this bridge. `declared` is the major
    /// its manifest named.
    ///
    /// A major that differs from the declaration, or that the page does not
    /// speak, is `unavailable (malformed)`: the shell can tell a module that
    /// speaks the bridge from one that does not (REQ-5.1). A second `ready`
    /// changes nothing.
    pub fn ready(&mut self, bridge: [u32; 2], declared: u32, now: f64) {
        if self.state != PanelState::Loading {
            return;
        }
        if bridge[0] != declared || !SUPPORTED_BRIDGE_MAJORS.contains(&bridge[0]) {
            self.state = PanelState::Unavailable(Cause::Malformed);
            return;
        }
        self.state = PanelState::Ready;
        self.last_beat_at = now;
        self.outstanding = None;
        self.misses = 0;
    }

    /// The module echoed heartbeat `n`.
    ///
    /// Only the one outstanding counts: an echo of an older one arrived after
    /// the next was already sent, which is exactly a heartbeat not echoed in
    /// time. A stale module answering recovers to `ready`.
    pub fn echoed(&mut self, n: u64) {
        if !matches!(self.state, PanelState::Ready | PanelState::Stale) {
            return;
        }
        if self.outstanding != Some(n) {
            return;
        }
        self.outstanding = None;
        self.misses = 0;
        self.state = PanelState::Ready;
    }

    /// The panel scrolled in or out of view, or the tab was shown or hidden.
    ///
    /// Browsers throttle timers in a hidden document, so a hidden module cannot
    /// be judged by its answers: nothing is sent while hidden, the heartbeat in
    /// flight is forgotten, and the first one after it is shown again judges it.
    pub fn visible(&mut self, visible: bool, now: f64) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        self.outstanding = None;
        if visible {
            // Due at the next tick rather than a whole interval from now.
            self.last_beat_at = now - HEARTBEAT_MS;
        }
    }

    /// Whether the panel is in view and the tab showing.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Something outside the bridge settled it: the entry document was refused
    /// or unreachable.
    pub fn failed(&mut self, cause: Cause) {
        if !matches!(self.state, PanelState::Unavailable(_)) {
            self.state = PanelState::Unavailable(cause);
        }
    }

    /// Time passed. Says what is owed to the module now, and moves the state
    /// on where the silence has lasted long enough.
    ///
    /// Called often (every [`INIT_RESEND_MS`]); it is a no-op when nothing is
    /// due. `unavailable` is terminal: a module recovers from it only by being
    /// mounted again, which is a new `Liveness`.
    pub fn tick(&mut self, now: f64) -> Option<Due> {
        match self.state {
            PanelState::Unavailable(_) => None,

            PanelState::Loading => {
                // From the load event, as the specification says, or from the
                // mount where the document has not loaded at all: a frame that
                // never loads is not left `loading` forever.
                let since = self.loaded_at.unwrap_or(self.mounted_at);
                if now - since >= READY_TIMEOUT_MS {
                    self.state = PanelState::Unavailable(Cause::Unreachable);
                    return None;
                }
                self.loaded_at?;
                if self
                    .last_init
                    .is_some_and(|sent| now - sent < INIT_RESEND_MS)
                {
                    return None;
                }
                self.last_init = Some(now);
                Some(Due::Init)
            }

            PanelState::Ready | PanelState::Stale => {
                if !self.visible || now - self.last_beat_at < HEARTBEAT_MS {
                    return None;
                }
                if self.outstanding.is_some() {
                    self.misses += 1;
                    if self.misses >= MISSES_UNTIL_UNREACHABLE {
                        self.state = PanelState::Unavailable(Cause::Unreachable);
                        return None;
                    }
                    self.state = PanelState::Stale;
                }
                self.beat += 1;
                self.outstanding = Some(self.beat);
                self.last_beat_at = now;
                Some(Due::Heartbeat(self.beat))
            }
        }
    }
}

/// Whether a panel stops showing its module and draws its data instead
/// (*Fallback*).
///
/// Only a panel that declares data has anything to fall back to, and only for
/// a reason about the module. `unknown` and `deprecated` are about the panel
/// itself, and drawing its data would pretend otherwise.
pub fn falls_back(state: PanelState, declares_data: bool) -> bool {
    declares_data
        && matches!(
            state,
            PanelState::Unavailable(Cause::Unreachable | Cause::Malformed)
        )
}

/// What the answer to the page's own request for a module's entry document
/// says about the module (*Panel states*).
///
/// `None` for an answer that is fine. The asset route chooses its statuses so
/// the class alone decides: any 4xx is the platform's module declared or built
/// wrongly, and anything else that is not success, including no answer at all
/// (status 0), is the platform not being there.
pub fn entry_cause(status: u16) -> Option<Cause> {
    match status {
        200..=299 | 304 => None,
        400..=499 => Some(Cause::Malformed),
        _ => Some(Cause::Unreachable),
    }
}

// -- Limits ------------------------------------------------------------------

/// What one frame may still do (*Limits*): how many messages a second, and how
/// many requests at once. Both are refused in the page, without a request.
#[derive(Debug, Clone, PartialEq)]
pub struct Allowance {
    per_second: u32,
    in_flight_at_most: u32,
    recent: VecDeque<f64>,
    in_flight: u32,
}

impl Allowance {
    /// A frame's allowance under these limits.
    pub fn new(per_second: u32, in_flight_at_most: u32) -> Self {
        Self {
            per_second: per_second.max(1),
            in_flight_at_most: in_flight_at_most.max(1),
            recent: VecDeque::new(),
            in_flight: 0,
        }
    }

    /// Whether a message arriving now is within the rate.
    ///
    /// Counted over the last second, sliding, so a burst at the end of one
    /// second and the start of the next is still one burst. Stream credit
    /// (`pull`) is not counted: the caller does not ask.
    pub fn admit(&mut self, now: f64) -> bool {
        while self.recent.front().is_some_and(|&at| now - at >= 1_000.0) {
            self.recent.pop_front();
        }
        if self.recent.len() >= self.per_second as usize {
            return false;
        }
        self.recent.push_back(now);
        true
    }

    /// Start a request, if another may be in flight.
    pub fn begin_fetch(&mut self) -> bool {
        if self.in_flight >= self.in_flight_at_most {
            return false;
        }
        self.in_flight += 1;
        true
    }

    /// A request finished, answered or not.
    pub fn end_fetch(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// Requests in flight.
    pub fn in_flight(&self) -> u32 {
        self.in_flight
    }
}

// -- The request proxy -------------------------------------------------------

/// Where the page sends a module's `fetch`: `/p/{platform}{path}?{query}`.
///
/// `platform` is the frame's, from the registry; the message never names one
/// (REQ-3.1). The shell checks the path again, but the page cannot leave it to
/// the shell: the browser resolves the address before anything is sent, and
/// resolving `/p/a/../../api/layouts` is `/api/layouts` — the shell's own API,
/// with the viewer's session. So anything a URL parser would read as a
/// different path is refused here: a dot segment in any spelling (`.`, `..`,
/// `%2e`, `.%2E`), a backslash (a separator in `http` URLs), and a `?` or `#`
/// inside the path.
pub fn proxy_address(platform: &str, path: &str, query: &str) -> Result<String, Refusal> {
    if !path.starts_with('/') || path.contains(['\\', '?', '#']) || query.contains('#') {
        return Err(Refusal::OutsidePrefix);
    }
    let dotted = path.split('/').any(|segment| {
        let spelled = segment.to_ascii_lowercase().replace("%2e", ".");
        spelled == "." || spelled == ".."
    });
    if dotted {
        return Err(Refusal::OutsidePrefix);
    }
    let mut address = format!("/p/{platform}{path}");
    let query = query.strip_prefix('?').unwrap_or(query);
    if !query.is_empty() {
        address.push('?');
        address.push_str(query);
    }
    Ok(address)
}

/// The status the shell uses for each of its refusals, so a refusal the page
/// makes itself is indistinguishable from one the shell would have made.
pub fn refusal_status(refusal: Refusal) -> u16 {
    match refusal {
        Refusal::NotFromShell | Refusal::ReadOnly => 403,
        Refusal::NotSignedIn => 401,
        Refusal::OutsidePrefix => 404,
        Refusal::Method => 405,
        Refusal::NoIdentity => 409,
        Refusal::NoIdempotencyKey => 400,
        Refusal::TooLarge => 413,
        Refusal::Unreachable => 502,
        Refusal::Timeout => 504,
        Refusal::TooMany => 429,
        Refusal::Unrecognised => 400,
    }
}

/// The refusal an `X-Hlin-Refusal` header names, if it names one.
///
/// A code this page does not know is still the shell's refusal, not the
/// platform's answer, so it comes back as [`Refusal::Unrecognised`] rather
/// than as nothing.
pub fn refusal_named(header: Option<&str>) -> Option<Refusal> {
    let code = header?.trim();
    if code.is_empty() {
        return None;
    }
    Some(serde_json::from_value(serde_json::Value::from(code)).unwrap_or(Refusal::Unrecognised))
}

/// The platform's response headers a module may see.
pub fn passed_back<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<String, String> {
    headers
        .into_iter()
        .filter(|(name, _)| {
            ALLOWED_RESPONSE_HEADERS
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(name))
        })
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
        .collect()
}

// -- What a module is told ---------------------------------------------------

/// A panel's parameters as its module is told them: only the ones the panel
/// declares (*Messages*, `init`; [[HLIN-T-0028]]).
///
/// A layout can hold a selection for a parameter the panel no longer declares
/// — the platform withdrew it, or a person typed it before it was a control —
/// and a module has no business hearing about it.
pub fn declared_only(
    selections: &hlin_bridge::Selections,
    declared: &std::collections::BTreeSet<String>,
) -> hlin_bridge::Selections {
    selections
        .iter()
        .filter(|(id, _)| declared.contains(*id))
        .map(|(id, values)| (id.clone(), values.clone()))
        .collect()
}

/// Whether a module should hear that its platform changed something.
///
/// Every module of the platform hears it, whichever panel it draws: the
/// message names the panel, and a module refetches if it cares (*Messages*,
/// `changed`). The one module that does not is one that has chosen a
/// different value for a parameter the change names. A checklist's `team`
/// list changing is nothing to a module showing `home`; a module with no list
/// chosen shows whatever its platform defaults to, which may be `team`, so it
/// hears it.
pub fn hears(change: &hlin_bridge::Selections, chosen: &hlin_bridge::Selections) -> bool {
    change
        .iter()
        .all(|(param, values)| chosen.get(param).is_none_or(|mine| mine == values))
}

/// A `notice`'s text as the shell shows it: plain, on one line, at most
/// [`hlin_bridge::NOTICE_MAX_CHARS`] characters (*Messages*, `notice`).
///
/// `None` for nothing to show, which clears the panel's notice. Control
/// characters and runs of whitespace become single spaces, so a platform
/// cannot push the chrome around with newlines; the text is drawn as text,
/// never markup, by the view. Longer text is cut and ends with an ellipsis, so
/// a person can see it was.
pub fn notice_text(text: &str) -> Option<String> {
    let plain = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if plain.is_empty() {
        return None;
    }
    let most = hlin_bridge::NOTICE_MAX_CHARS;
    if plain.chars().count() <= most {
        return Some(plain);
    }
    let mut cut: String = plain.chars().take(most - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    Some(cut)
}

/// The chrome's colour roles a module is sent in `theme`: the properties a
/// design pack fills for the shell's own frame (`DesignPack::stylesheet`).
///
/// These rather than a vocabulary of the bridge's own, because they are what
/// a pack already answers for, so a module drawn inside a panel can match the
/// frame around it with whatever pack is mounted.
pub const THEME_TOKENS: [&str; 11] = [
    "--hlin-surface",
    "--hlin-raised",
    "--hlin-border",
    "--hlin-text",
    "--hlin-dim",
    "--hlin-faint",
    "--hlin-accent",
    "--hlin-on-accent",
    "--hlin-good",
    "--hlin-warn",
    "--hlin-bad",
];

/// Light or dark, as the page actually looks.
///
/// Read from the page's own surface colour rather than from the operating
/// system's preference, because the mounted pack decides: Aurora Dark is dark
/// in a light-mode browser. The preference is the answer only when the colour
/// cannot be read.
pub fn scheme_of(surface: &str, prefers_dark: bool) -> hlin_bridge::Scheme {
    match luminance(surface) {
        Some(light) if light < 0.5 => hlin_bridge::Scheme::Dark,
        Some(_) => hlin_bridge::Scheme::Light,
        None if prefers_dark => hlin_bridge::Scheme::Dark,
        None => hlin_bridge::Scheme::Light,
    }
}

/// Relative luminance from 0 to 1 of a colour written as a browser computes
/// custom properties: `#rgb`, `#rrggbb`, `#rrggbbaa`, or `rgb()`/`rgba()`.
fn luminance(colour: &str) -> Option<f64> {
    let colour = colour.trim();
    let (r, g, b) = if let Some(hex) = colour.strip_prefix('#') {
        let digit = |at: usize, width: usize| {
            u8::from_str_radix(hex.get(at..at + width)?, 16)
                .ok()
                .map(|value| if width == 1 { value * 17 } else { value })
        };
        match hex.len() {
            3 | 4 => (digit(0, 1)?, digit(1, 1)?, digit(2, 1)?),
            6 | 8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?),
            _ => return None,
        }
    } else {
        let inner = colour
            .strip_prefix("rgba(")
            .or_else(|| colour.strip_prefix("rgb("))?
            .strip_suffix(')')?;
        let mut parts = inner
            .split([',', ' ', '/'])
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<f64>().ok().map(|v| v.clamp(0.0, 255.0) as u8));
        (parts.next()??, parts.next()??, parts.next()??)
    };
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b))
}

// -- Budget ------------------------------------------------------------------

/// How many frames a surface keeps mounted (*Budget*).
pub const FRAME_BUDGET: usize = 12;

/// A mounted frame, as the budget weighs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mounted {
    /// The panel instance.
    pub instance: String,
    /// In the viewport now. Never unmounted.
    pub in_view: bool,
    /// Within the mounting margin of the viewport: a person is about to see
    /// it, so it goes after one that is further away.
    pub near: bool,
    /// When it was last in view, in milliseconds.
    pub last_seen: f64,
    /// Already asked to `suspend`, and on its way out. Still in the document,
    /// so still counted; but already going, so not chosen again.
    pub leaving: bool,
}

/// Which frames to unmount so the surface is back within `budget`, first
/// first (REQ-4.3).
///
/// Every frame in the document counts, including one being suspended: it is
/// there until it has gone, and a budget that stopped counting it at `suspend`
/// ran to eighteen while scrolling ([[HLIN-T-0085]]). One already leaving is
/// not chosen again, and the frames already leaving make up that much of the
/// excess.
///
/// The least recently seen of the frames out of view, and of those, one far
/// from the viewport before one about to scroll in. Never one in view: a
/// surface with more than the budget in view at once runs over it rather than
/// blank what a person is looking at.
pub fn over_budget(mounted: &[Mounted], budget: usize) -> Vec<String> {
    let leaving = mounted.iter().filter(|frame| frame.leaving).count();
    let excess = mounted.len().saturating_sub(budget).saturating_sub(leaving);
    let mut candidates: Vec<&Mounted> = mounted
        .iter()
        .filter(|frame| !frame.in_view && !frame.leaving)
        .collect();
    candidates.sort_by(|a, b| {
        a.near
            .cmp(&b.near)
            .then(a.last_seen.total_cmp(&b.last_seen))
            .then(a.instance.cmp(&b.instance))
    });
    candidates
        .into_iter()
        .take(excess)
        .map(|frame| frame.instance.clone())
        .collect()
}

/// Whether one more frame may be put in the document now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Room {
    /// Yes: there is room, or the newcomer is in view and nothing else can go.
    Now,
    /// Not yet. These go first (asked to `suspend`, or taken at once), and the
    /// newcomer is mounted once one of them, or one already leaving, has left
    /// the document. Empty when nothing can go and the newcomer is not in
    /// view: it waits until it is.
    After(Vec<String>),
}

/// Whether a frame not yet in the document may be put there, given the frames
/// that are (*Budget*).
///
/// Room is made before the frame is mounted rather than after, so the count
/// in the document never passes `budget` on the way: a frame is suspended
/// and gone first, then the newcomer arrives. Only a frame in view is mounted
/// past the budget, and only when nothing out of view is left to take.
pub fn room_for(mounted: &[Mounted], budget: usize, in_view: bool) -> Room {
    if mounted.len() < budget {
        return Room::Now;
    }
    let going = over_budget(mounted, budget.saturating_sub(1));
    let leaving = mounted.iter().any(|frame| frame.leaving);
    if going.is_empty() && !leaving && in_view {
        return Room::Now;
    }
    Room::After(going)
}

/// Whether a `state` blob is one the page keeps: at most `state_bytes`.
/// A larger one is dropped whole, and the module starts fresh when it
/// remounts, which is what a module that ignored `suspend` gets anyway.
pub fn keeps_state(bytes: usize, state_bytes: u64) -> bool {
    bytes as u64 <= state_bytes
}

// -- Streams -------------------------------------------------------------------

/// Module streams a page served over HTTP/1.1 holds open at once
/// (*Streaming*, *Connections*).
///
/// A browser opens about six connections to one origin over HTTP/1.1, and
/// every stream holds one for as long as it runs. Four leaves the shell's own
/// event stream and an ordinary request or two room to move.
pub const PAGE_STREAMS_HTTP1: usize = 4;

/// Module streams a page served over HTTP/2 or later holds open at once,
/// where every request shares one connection.
pub const PAGE_STREAMS_MULTIPLEXED: usize = 32;

/// How many module streams this page may hold open, from the protocol its
/// own navigation was served over (`nextHopProtocol`).
///
/// Anything but a protocol known to multiplex gets the HTTP/1.1 cap,
/// including an empty string, which is what a browser reports when it will
/// not say: guessing wrong that way costs a module a stream, and guessing
/// wrong the other way stalls the whole page.
pub fn page_stream_cap(next_hop_protocol: &str) -> usize {
    let protocol = next_hop_protocol.trim().to_ascii_lowercase();
    if protocol.starts_with("h2") || protocol.starts_with("h3") {
        PAGE_STREAMS_MULTIPLEXED
    } else {
        PAGE_STREAMS_HTTP1
    }
}

/// Whether one more stream may open: within its frame's `streams` and within
/// the page's cap. Past either it is refused `too_many`.
pub fn admits_stream(
    open_in_frame: usize,
    frame_streams: u32,
    open_on_page: usize,
    page_cap: usize,
) -> bool {
    open_in_frame < frame_streams as usize && open_on_page < page_cap
}

/// Why a stream ended, as the module is told, from why the shell said it
/// ended. `None` is a stream that finished.
///
/// A reason the page does not know is still a stream that failed, and the
/// nearest thing the module can act on is that the platform could not be
/// followed to the end.
pub fn end_error(ended: hlin_stream::streamed::Ended) -> Option<hlin_bridge::EndError> {
    use hlin_bridge::EndError;
    use hlin_stream::streamed::Ended;
    match ended {
        Ended::Finished => None,
        Ended::Idle => Some(EndError::Idle),
        Ended::Rate => Some(EndError::Rate),
        Ended::Unreachable | Ended::Unrecognised => Some(EndError::Unreachable),
    }
}

/// A streamed body between the page's reads and the module's `pull`s.
///
/// The page sends only what the module has asked for (credit), in order, a
/// piece split where the credit runs out, and reads more from the network
/// only once what it holds is sent and there is credit left. So what it holds
/// is never more than one read, and a module that stops pulling stops the
/// page reading.
#[derive(Debug, Default)]
pub struct Outbox {
    credit: u64,
    held: std::collections::VecDeque<Vec<u8>>,
    seq: u64,
    delivered: u64,
}

impl Outbox {
    /// Nothing held, and no credit.
    pub fn new() -> Self {
        Self::default()
    }

    /// The module asked for `bytes` more.
    pub fn grant(&mut self, bytes: u64) {
        self.credit = self.credit.saturating_add(bytes);
    }

    /// The page read this from the network.
    pub fn hold(&mut self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            self.held.push_back(bytes);
        }
    }

    /// The next `chunk` to send, with its `seq`, if there is anything held
    /// and any credit to send it with.
    pub fn next_chunk(&mut self) -> Option<(u64, Vec<u8>)> {
        if self.credit == 0 {
            return None;
        }
        let front = self.held.front_mut()?;
        let take = usize::try_from(self.credit)
            .unwrap_or(usize::MAX)
            .min(front.len());
        let chunk = if take == front.len() {
            self.held.pop_front()?
        } else {
            let rest = front.split_off(take);
            std::mem::replace(front, rest)
        };
        self.credit -= chunk.len() as u64;
        self.delivered += chunk.len() as u64;
        let seq = self.seq;
        self.seq += 1;
        Some((seq, chunk))
    }

    /// Whether the page should read more: nothing held, and credit to spend.
    pub fn wants_more(&self) -> bool {
        self.held.is_empty() && self.credit > 0
    }

    /// Whether everything read has been sent.
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// How many bytes have been sent to the module.
    pub fn delivered(&self) -> u64 {
        self.delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(instance: &str, platform: &str) -> Registered {
        Registered {
            platform: platform.to_string(),
            panel: "items".to_string(),
            instance: instance.to_string(),
            mounted_at: 0.0,
        }
    }

    // -- The registry --

    #[test]
    fn a_message_is_attributed_to_the_frame_whose_window_sent_it() {
        let mut registry = Registry::new();
        registry.register("window-a", frame("a", "checklist"));
        registry.register("window-b", frame("b", "feed"));

        assert_eq!(
            registry.source(&"window-b").map(|f| f.platform.as_str()),
            Some("feed")
        );
        assert_eq!(
            registry.source(&"window-a").map(|f| f.instance.as_str()),
            Some("a")
        );
    }

    #[test]
    fn a_window_the_page_did_not_create_is_nobody() {
        let mut registry = Registry::new();
        registry.register("window-a", frame("a", "checklist"));
        assert_eq!(registry.source(&"some-other-tab"), None);
    }

    #[test]
    fn a_frame_removed_from_the_registry_is_no_longer_heard() {
        let mut registry = Registry::new();
        registry.register("window-a", frame("a", "checklist"));

        let removed = registry.remove("a").expect("it was registered");
        assert_eq!(removed.instance, "a");
        assert_eq!(
            registry.source(&"window-a"),
            None,
            "a message still in flight from a torn-down frame is dropped"
        );
        assert!(registry.is_empty());
        assert_eq!(registry.remove("a"), None);
    }

    #[test]
    fn remounting_an_instance_leaves_only_the_new_window_able_to_speak_for_it() {
        let mut registry = Registry::new();
        registry.register("old", frame("a", "checklist"));
        registry.register("new", frame("a", "checklist"));

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.source(&"old"), None);
        assert!(registry.source(&"new").is_some());
        assert_eq!(registry.window("a"), Some(&"new"));
    }

    // -- Liveness --

    /// A module that loaded at 0 and said `ready` at 100.
    fn ready_at_100() -> Liveness {
        let mut live = Liveness::new(0.0);
        live.loaded(0.0);
        assert_eq!(live.tick(0.0), Some(Due::Init));
        live.ready([1, 0], 1, 100.0);
        assert_eq!(live.state(), PanelState::Ready);
        live
    }

    #[test]
    fn init_waits_for_the_frame_to_load_then_is_resent_until_ready() {
        let mut live = Liveness::new(0.0);
        assert_eq!(live.tick(10.0), None, "nothing to send into an empty frame");
        assert_eq!(live.state(), PanelState::Loading);

        live.loaded(50.0);
        assert_eq!(live.tick(50.0), Some(Due::Init));
        assert_eq!(live.tick(200.0), None, "not every tick");
        assert_eq!(
            live.tick(300.0),
            Some(Due::Init),
            "again after 250 ms, in case nobody was listening yet"
        );

        live.ready([1, 0], 1, 400.0);
        assert_eq!(live.tick(700.0), None, "and never once it is ready");
    }

    #[test]
    fn no_ready_within_ten_seconds_of_loading_is_unreachable() {
        let mut live = Liveness::new(0.0);
        live.loaded(1_000.0);
        live.tick(1_000.0);
        live.tick(10_999.0);
        assert_eq!(
            live.state(),
            PanelState::Loading,
            "the clock starts at load"
        );
        assert_eq!(live.tick(11_000.0), None);
        assert_eq!(live.state(), PanelState::Unavailable(Cause::Unreachable));
    }

    #[test]
    fn a_frame_that_never_loads_is_not_loading_forever() {
        let mut live = Liveness::new(0.0);
        live.tick(10_000.0);
        assert_eq!(live.state(), PanelState::Unavailable(Cause::Unreachable));
    }

    #[test]
    fn ready_naming_another_major_is_malformed() {
        let mut live = Liveness::new(0.0);
        live.loaded(0.0);
        live.ready([2, 0], 1, 10.0);
        assert_eq!(live.state(), PanelState::Unavailable(Cause::Malformed));

        // And a major the manifest declared but this page does not speak.
        let mut live = Liveness::new(0.0);
        live.ready([7, 0], 7, 10.0);
        assert_eq!(live.state(), PanelState::Unavailable(Cause::Malformed));
    }

    #[test]
    fn a_newer_minor_of_the_same_major_is_ready() {
        let mut live = Liveness::new(0.0);
        live.ready([1, 9], 1, 10.0);
        assert_eq!(live.state(), PanelState::Ready);
    }

    #[test]
    fn a_module_that_echoes_every_heartbeat_stays_ready() {
        let mut live = ready_at_100();
        assert_eq!(
            live.tick(1_000.0),
            None,
            "the first is two seconds after ready"
        );

        let mut now = 2_100.0;
        for n in 1..=5 {
            assert_eq!(live.tick(now), Some(Due::Heartbeat(n)));
            live.echoed(n);
            assert_eq!(live.state(), PanelState::Ready);
            now += HEARTBEAT_MS;
        }
    }

    #[test]
    fn one_miss_is_stale_three_in_a_row_unreachable() {
        let mut live = ready_at_100();
        assert_eq!(live.tick(2_100.0), Some(Due::Heartbeat(1)));
        // Not echoed.
        assert_eq!(live.tick(4_100.0), Some(Due::Heartbeat(2)));
        assert_eq!(live.state(), PanelState::Stale, "one miss dims it");
        assert_eq!(live.tick(6_100.0), Some(Due::Heartbeat(3)));
        assert_eq!(live.state(), PanelState::Stale);
        assert_eq!(live.tick(8_100.0), None);
        assert_eq!(
            live.state(),
            PanelState::Unavailable(Cause::Unreachable),
            "three in a row, about six seconds, and it is gone"
        );
        assert_eq!(live.tick(20_000.0), None, "and nothing more is sent");
        live.echoed(3);
        assert_eq!(
            live.state(),
            PanelState::Unavailable(Cause::Unreachable),
            "unreachable recovers only by remounting"
        );
    }

    #[test]
    fn a_stale_module_that_answers_again_is_ready() {
        let mut live = ready_at_100();
        live.tick(2_100.0);
        live.tick(4_100.0);
        assert_eq!(live.state(), PanelState::Stale);
        live.echoed(2);
        assert_eq!(live.state(), PanelState::Ready);

        // And the count of misses started again.
        live.tick(6_100.0);
        live.tick(8_100.0);
        assert_eq!(live.state(), PanelState::Stale);
    }

    #[test]
    fn a_late_echo_of_an_older_heartbeat_does_not_count() {
        let mut live = ready_at_100();
        live.tick(2_100.0);
        live.tick(4_100.0);
        live.echoed(1);
        assert_eq!(
            live.state(),
            PanelState::Stale,
            "heartbeat 1 was not echoed within two seconds, however late it came"
        );
    }

    #[test]
    fn no_heartbeat_while_hidden_and_the_first_after_judges_it() {
        let mut live = ready_at_100();
        live.tick(2_100.0);
        live.visible(false, 2_500.0);

        for now in [4_100.0, 6_100.0, 8_100.0, 30_000.0] {
            assert_eq!(live.tick(now), None, "nothing is sent to a hidden frame");
        }
        assert_eq!(
            live.state(),
            PanelState::Ready,
            "and a hidden module is not judged by the silence"
        );

        live.visible(true, 31_000.0);
        assert_eq!(live.tick(31_000.0), Some(Due::Heartbeat(2)), "at once");
        live.echoed(2);
        assert_eq!(live.state(), PanelState::Ready);
    }

    #[test]
    fn an_entry_that_could_not_be_had_settles_it() {
        let mut live = Liveness::new(0.0);
        live.failed(Cause::Malformed);
        assert_eq!(live.state(), PanelState::Unavailable(Cause::Malformed));
        live.failed(Cause::Unreachable);
        assert_eq!(
            live.state(),
            PanelState::Unavailable(Cause::Malformed),
            "the first cause stands"
        );
    }

    #[test]
    fn entry_statuses_split_by_class() {
        assert_eq!(entry_cause(200), None);
        assert_eq!(entry_cause(304), None);
        assert_eq!(entry_cause(404), Some(Cause::Malformed));
        assert_eq!(entry_cause(413), Some(Cause::Malformed));
        assert_eq!(entry_cause(502), Some(Cause::Unreachable));
        assert_eq!(entry_cause(504), Some(Cause::Unreachable));
        assert_eq!(entry_cause(0), Some(Cause::Unreachable));
    }

    // -- Fallback --

    #[test]
    fn a_panel_falls_back_only_for_a_reason_about_its_module() {
        use Cause::*;
        assert!(falls_back(PanelState::Unavailable(Unreachable), true));
        assert!(falls_back(PanelState::Unavailable(Malformed), true));
        assert!(!falls_back(PanelState::Unavailable(Unknown), true));
        assert!(!falls_back(PanelState::Unavailable(Deprecated), true));
        assert!(!falls_back(PanelState::Stale, true), "stale only dims it");
        assert!(!falls_back(PanelState::Loading, true));
        assert!(
            !falls_back(PanelState::Unavailable(Unreachable), false),
            "a panel with no data has nothing to fall back to"
        );
    }

    // -- Limits --

    #[test]
    fn messages_past_the_rate_are_refused_until_the_second_has_passed() {
        let mut allowance = Allowance::new(3, 8);
        assert!(allowance.admit(0.0));
        assert!(allowance.admit(10.0));
        assert!(allowance.admit(20.0));
        assert!(!allowance.admit(30.0));
        assert!(!allowance.admit(999.0));
        assert!(allowance.admit(1_000.0), "the first has aged out");
        assert!(!allowance.admit(1_001.0));
    }

    #[test]
    fn requests_past_the_in_flight_limit_are_refused_until_one_finishes() {
        let mut allowance = Allowance::new(50, 2);
        assert!(allowance.begin_fetch());
        assert!(allowance.begin_fetch());
        assert!(!allowance.begin_fetch());
        assert_eq!(allowance.in_flight(), 2);
        allowance.end_fetch();
        assert!(allowance.begin_fetch());
        allowance.end_fetch();
        allowance.end_fetch();
        allowance.end_fetch();
        assert_eq!(allowance.in_flight(), 0, "never below nothing");
    }

    // -- The request proxy --

    #[test]
    fn a_fetch_goes_to_the_frames_own_platform() {
        assert_eq!(
            proxy_address("checklist", "/api/lists/team/items", "").as_deref(),
            Ok("/p/checklist/api/lists/team/items")
        );
        assert_eq!(
            proxy_address("checklist", "/api/items", "list=team&n=2").as_deref(),
            Ok("/p/checklist/api/items?list=team&n=2")
        );
        assert_eq!(
            proxy_address("checklist", "/api/items", "?list=team").as_deref(),
            Ok("/p/checklist/api/items?list=team"),
            "a query written with its question mark is the same query"
        );
    }

    #[test]
    fn a_path_the_browser_would_resolve_elsewhere_is_refused_before_it_is_sent() {
        for path in [
            "/../../api/layouts",
            "/api/../../../api/layouts",
            "/./api",
            "/%2e%2e/%2e%2e/api/layouts",
            "/.%2E/api",
            "/%2E./api",
            "/api\\..\\..\\api",
            "/api?x=1",
            "/api#fragment",
            "api/no-leading-slash",
            "//evil.example.com/api",
        ] {
            let refused = proxy_address("checklist", path, "");
            if path.starts_with("//") {
                // Not a separator trick: it stays under `/p/checklist/`, and
                // the shell refuses the empty segment.
                assert_eq!(refused.as_deref(), Ok("/p/checklist//evil.example.com/api"));
                continue;
            }
            assert_eq!(refused, Err(Refusal::OutsidePrefix), "{path}");
        }
        assert_eq!(
            proxy_address("checklist", "/api", "a=1#b"),
            Err(Refusal::OutsidePrefix)
        );
    }

    #[test]
    fn dots_inside_a_segment_are_ordinary_characters() {
        assert!(proxy_address("checklist", "/api/v1.2/items..json", "").is_ok());
    }

    #[test]
    fn the_pages_refusals_carry_the_shells_statuses() {
        assert_eq!(refusal_status(Refusal::TooMany), 429);
        assert_eq!(refusal_status(Refusal::OutsidePrefix), 404);
        assert_eq!(refusal_status(Refusal::Method), 405);
        assert_eq!(refusal_status(Refusal::TooLarge), 413);
        assert_eq!(refusal_status(Refusal::Unreachable), 502);
    }

    #[test]
    fn the_shells_refusal_header_is_read_and_an_unknown_code_is_still_a_refusal() {
        assert_eq!(refusal_named(None), None);
        assert_eq!(refusal_named(Some("")), None);
        assert_eq!(refusal_named(Some("read_only")), Some(Refusal::ReadOnly));
        assert_eq!(
            refusal_named(Some("no_idempotency_key")),
            Some(Refusal::NoIdempotencyKey)
        );
        assert_eq!(
            refusal_named(Some("from_the_future")),
            Some(Refusal::Unrecognised)
        );
    }

    #[test]
    fn only_the_allowed_response_headers_reach_a_module() {
        let passed = passed_back([
            ("Content-Type", "application/json"),
            ("ETag", "\"7\""),
            ("set-cookie", "session=stolen"),
            ("x-hlin-refusal", "too_many"),
        ]);
        assert_eq!(
            passed.keys().map(String::as_str).collect::<Vec<_>>(),
            ["content-type", "etag"]
        );
    }

    // -- What a module is told --

    fn chosen(pairs: &[(&str, &[&str])]) -> hlin_bridge::Selections {
        pairs
            .iter()
            .map(|(id, values)| {
                (
                    id.to_string(),
                    values.iter().map(|v| v.to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_module_is_told_only_the_parameters_its_panel_declares() {
        let stored = chosen(&[("cluster", &["west"]), ("withdrawn", &["x"])]);
        let declared = ["cluster".to_string()].into();
        assert_eq!(
            declared_only(&stored, &declared),
            chosen(&[("cluster", &["west"])])
        );
    }

    #[test]
    fn a_change_reaches_every_module_of_the_platform_but_one_that_chose_otherwise() {
        let team = chosen(&[("list", &["team"])]);
        assert!(hears(&team, &chosen(&[])), "a module that chose nothing");
        assert!(hears(&team, &team), "a module on that list");
        assert!(
            hears(&team, &chosen(&[("colour", &["red"])])),
            "a module that chose only something the change does not name"
        );
        assert!(
            !hears(&team, &chosen(&[("list", &["home"])])),
            "a module showing another list"
        );
        assert!(
            hears(&chosen(&[]), &chosen(&[("list", &["home"])])),
            "a change naming nothing is everyone's"
        );
    }

    #[test]
    fn a_notice_is_one_plain_line_of_at_most_a_hundred_and_forty_characters() {
        assert_eq!(
            notice_text("  Sync\npaused\t\u{7}now "),
            Some("Sync paused now".into())
        );
        assert_eq!(notice_text(" \n "), None, "nothing to show clears it");

        let long = "word ".repeat(60);
        let shown = notice_text(&long).unwrap();
        assert_eq!(shown.chars().count(), 140);
        assert!(shown.ends_with("word…"), "{shown}");
        let spaced = notice_text(&"abc ".repeat(60)).unwrap();
        assert!(
            spaced.ends_with("abc…"),
            "no space before the ellipsis: {spaced}"
        );

        let exact = "a".repeat(140);
        assert_eq!(notice_text(&exact), Some(exact.clone()));
        let over = "é".repeat(141);
        assert_eq!(notice_text(&over).unwrap().chars().count(), 140);
    }

    #[test]
    fn the_scheme_is_read_from_the_page_the_pack_drew_not_the_system() {
        use hlin_bridge::Scheme;
        assert_eq!(scheme_of("#0f1115", false), Scheme::Dark);
        assert_eq!(scheme_of(" #f8f9fa", true), Scheme::Light);
        assert_eq!(scheme_of("#fff", true), Scheme::Light);
        assert_eq!(scheme_of("#111a", false), Scheme::Dark);
        assert_eq!(scheme_of("rgb(15, 17, 21)", false), Scheme::Dark);
        assert_eq!(scheme_of("rgba(250 250 250 / 1)", true), Scheme::Light);
        assert_eq!(
            scheme_of("", true),
            Scheme::Dark,
            "unreadable: ask the system"
        );
        assert_eq!(scheme_of("canvas", false), Scheme::Light);
    }

    // -- Budget --

    fn mounted(instance: &str, in_view: bool, near: bool, last_seen: f64) -> Mounted {
        Mounted {
            instance: instance.into(),
            in_view,
            near,
            last_seen,
            leaving: false,
        }
    }

    fn leaving(instance: &str) -> Mounted {
        Mounted {
            leaving: true,
            ..mounted(instance, false, false, 0.0)
        }
    }

    #[test]
    fn a_frame_being_suspended_counts_until_it_has_gone_and_is_not_asked_twice() {
        let mut frames: Vec<_> = (0..11)
            .map(|i| mounted(&format!("f{i}"), false, false, f64::from(i)))
            .collect();
        frames.push(leaving("going"));
        frames.push(mounted("new", true, true, 100.0));
        assert!(
            over_budget(&frames, FRAME_BUDGET).is_empty(),
            "thirteen, one of them already leaving: nothing more to take"
        );
        frames.push(mounted("newer", true, true, 100.0));
        assert_eq!(over_budget(&frames, FRAME_BUDGET), ["f0"]);
    }

    #[test]
    fn room_is_made_before_a_frame_is_mounted_not_after() {
        let eleven: Vec<_> = (0..11)
            .map(|i| mounted(&format!("f{i}"), false, false, f64::from(i)))
            .collect();
        assert_eq!(room_for(&eleven, FRAME_BUDGET, false), Room::Now);

        let mut twelve = eleven.clone();
        twelve.push(mounted("f11", true, false, 50.0));
        assert_eq!(
            room_for(&twelve, FRAME_BUDGET, true),
            Room::After(vec!["f0".into()]),
            "at the budget, the least recently seen goes first, even for a frame in view"
        );

        let mut going = eleven.clone();
        going.push(leaving("going"));
        assert_eq!(
            room_for(&going, FRAME_BUDGET, true),
            Room::After(vec![]),
            "one already leaving makes the room: wait for it"
        );
    }

    #[test]
    fn a_frame_in_view_is_mounted_over_budget_only_when_nothing_else_can_go() {
        let in_view: Vec<_> = (0..12)
            .map(|i| mounted(&format!("f{i}"), true, false, 0.0))
            .collect();
        assert_eq!(room_for(&in_view, FRAME_BUDGET, true), Room::Now);
        assert_eq!(
            room_for(&in_view, FRAME_BUDGET, false),
            Room::After(vec![]),
            "one only near the viewport waits until it is in it"
        );
    }

    #[test]
    fn within_budget_nothing_is_unmounted() {
        let frames: Vec<_> = (0..12)
            .map(|i| mounted(&i.to_string(), false, false, 0.0))
            .collect();
        assert!(over_budget(&frames, FRAME_BUDGET).is_empty());
    }

    #[test]
    fn past_budget_the_least_recently_seen_frame_out_of_view_goes_first() {
        let mut frames: Vec<_> = (0..12)
            .map(|i| mounted(&format!("f{i}"), true, true, 100.0))
            .collect();
        frames.push(mounted("seen-long-ago", false, false, 10.0));
        frames.push(mounted("seen-lately", false, false, 90.0));
        assert_eq!(
            over_budget(&frames, FRAME_BUDGET),
            ["seen-long-ago", "seen-lately"]
        );
        assert_eq!(over_budget(&frames[..13], FRAME_BUDGET), ["seen-long-ago"]);
    }

    #[test]
    fn a_frame_about_to_scroll_in_outlasts_one_far_away_however_recently_seen() {
        let mut frames: Vec<_> = (0..12)
            .map(|i| mounted(&format!("f{i}"), true, true, 100.0))
            .collect();
        frames.push(mounted("just-below", false, true, 1.0));
        frames.push(mounted("far-away", false, false, 50.0));
        assert_eq!(over_budget(&frames[..], 13), ["far-away"]);
    }

    #[test]
    fn a_frame_in_view_is_never_unmounted_to_meet_the_budget() {
        let frames: Vec<_> = (0..15)
            .map(|i| mounted(&format!("f{i}"), true, false, 0.0))
            .collect();
        assert!(
            over_budget(&frames, FRAME_BUDGET).is_empty(),
            "fifteen in view is over budget, and stays so"
        );

        let mut frames = frames;
        frames.push(mounted("below", false, false, 5.0));
        assert_eq!(over_budget(&frames, FRAME_BUDGET), ["below"]);
    }

    #[test]
    fn a_state_blob_is_kept_only_within_its_limit() {
        assert!(keeps_state(0, 64 * 1024));
        assert!(keeps_state(64 * 1024, 64 * 1024));
        assert!(!keeps_state(64 * 1024 + 1, 64 * 1024));
    }

    // -- Streams --

    #[test]
    fn the_page_holds_more_streams_only_over_a_protocol_that_multiplexes() {
        assert_eq!(page_stream_cap("http/1.1"), PAGE_STREAMS_HTTP1);
        assert_eq!(page_stream_cap("http/1.0"), PAGE_STREAMS_HTTP1);
        assert_eq!(page_stream_cap("h2"), PAGE_STREAMS_MULTIPLEXED);
        assert_eq!(page_stream_cap("h3"), PAGE_STREAMS_MULTIPLEXED);
        assert_eq!(page_stream_cap("h3-29"), PAGE_STREAMS_MULTIPLEXED);
        // A browser that will not say is treated as the worst case.
        assert_eq!(page_stream_cap(""), PAGE_STREAMS_HTTP1);
        assert_eq!((PAGE_STREAMS_HTTP1, PAGE_STREAMS_MULTIPLEXED), (4, 32));
    }

    #[test]
    fn a_stream_opens_only_within_its_frames_limit_and_the_pages() {
        assert!(admits_stream(0, 2, 0, 4));
        assert!(admits_stream(1, 2, 3, 4));
        assert!(!admits_stream(2, 2, 2, 4), "the frame's own limit");
        assert!(!admits_stream(0, 2, 4, 4), "the page's cap");
        assert!(!admits_stream(0, 0, 0, 4), "streams switched off");
    }

    #[test]
    fn a_stream_is_ended_for_the_module_as_the_shell_ended_it() {
        use hlin_bridge::EndError;
        use hlin_stream::streamed::Ended;
        assert_eq!(end_error(Ended::Finished), None);
        assert_eq!(end_error(Ended::Idle), Some(EndError::Idle));
        assert_eq!(end_error(Ended::Rate), Some(EndError::Rate));
        assert_eq!(end_error(Ended::Unreachable), Some(EndError::Unreachable));
        assert_eq!(end_error(Ended::Unrecognised), Some(EndError::Unreachable));
    }

    #[test]
    fn nothing_is_sent_to_a_module_that_has_not_asked() {
        let mut outbox = Outbox::new();
        outbox.hold(b"data: one\n\n".to_vec());
        assert_eq!(outbox.next_chunk(), None);
        assert!(!outbox.wants_more(), "and nothing more is read");
        assert_eq!(outbox.delivered(), 0);
    }

    #[test]
    fn a_module_is_sent_exactly_what_it_asked_for_in_order() {
        let mut outbox = Outbox::new();
        outbox.grant(4);
        assert!(outbox.wants_more());
        outbox.hold(b"abcdef".to_vec());
        outbox.hold(b"gh".to_vec());

        assert_eq!(outbox.next_chunk(), Some((0, b"abcd".to_vec())));
        assert_eq!(outbox.next_chunk(), None, "credit spent");
        assert!(!outbox.wants_more());

        outbox.grant(100);
        assert_eq!(outbox.next_chunk(), Some((1, b"ef".to_vec())));
        assert_eq!(outbox.next_chunk(), Some((2, b"gh".to_vec())));
        assert_eq!(outbox.next_chunk(), None, "nothing held");
        assert!(outbox.is_empty());
        assert!(outbox.wants_more(), "credit left, so read more");
        assert_eq!(outbox.delivered(), 8);
    }

    #[test]
    fn credit_adds_up_and_does_not_overflow() {
        let mut outbox = Outbox::new();
        outbox.grant(u64::MAX);
        outbox.grant(u64::MAX);
        outbox.hold(vec![1; 10]);
        assert_eq!(outbox.next_chunk(), Some((0, vec![1; 10])));
    }
}
