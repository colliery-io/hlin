//! A debug build's watch on the main thread (HLIN-T-0090).
//!
//! A module must never block its main thread (specification HLIN-S-0007,
//! *Open Questions*): outside full Chromium a module's frame shares the page's
//! thread, so a module that spins holds the whole surface still, heartbeat and
//! all. This tells the module's author, in their own console, when theirs
//! does.
//!
//! Where the browser reports long tasks (Chromium), each of the frame's own
//! tasks is measured. Where it does not (Firefox, WebKit), a clock ticks every
//! [`TICK_MILLIS`] and a tick that comes late says the thread was held; it
//! cannot say by whom, since in those browsers the page and every module share
//! the thread, and the warning says so.
//!
//! Compiled into a debug build only: `lib.rs` declares this module under
//! `cfg(debug_assertions)`, so a release build carries none of it, observes
//! nothing and logs nothing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{
    PerformanceEntry, PerformanceObserver, PerformanceObserverEntryList, PerformanceObserverInit,
    Window,
};

/// A task at least this long is warned about.
///
/// The web calls a task long from 50 ms, but a debug build is unoptimised,
/// often several times slower than the release it stands for, and a warning
/// that fires on ordinary work in development is one people learn to ignore.
/// 200 ms is twice the 100 ms within which a person expects a click to be
/// answered: whatever holds the thread that long is felt in a release build
/// too, and is a mistake in the module's shape rather than its speed.
pub(crate) const THRESHOLD_MILLIS: f64 = 200.0;

/// After a warning, the next waits at least this long. Long tasks in the
/// meantime are counted into it, so a module that is always busy gets one
/// line every ten seconds, not one per task.
pub(crate) const QUIET_MILLIS: f64 = 10_000.0;

/// How often the fallback's clock ticks.
pub(crate) const TICK_MILLIS: i32 = 50;

/// The rule, as every warning ends.
pub(crate) const RULE: &str = "A module must never block its main thread: break long work into \
    chunks that yield, or move it to a Web Worker served from the module's own assets. See the \
    hlin-module crate docs, \"Never block the main thread\".";

/// How a long task was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// The browser measured one of this frame's own tasks.
    Measured,
    /// A clock ticked late: something held the thread, not necessarily this
    /// module.
    Late,
}

/// Decides which long tasks are warned about, and words the warning.
#[derive(Debug, Default)]
pub(crate) struct Warner {
    last_warned: Option<f64>,
    held: u32,
    longest: f64,
}

impl Warner {
    /// A task of `millis` ended at `now` (both on the frame's monotonic
    /// clock). The warning to log, if any.
    pub(crate) fn held(&mut self, now: f64, millis: f64, seen: Seen) -> Option<String> {
        if millis < THRESHOLD_MILLIS {
            return None;
        }
        if let Some(last) = self.last_warned
            && now - last < QUIET_MILLIS
        {
            self.held += 1;
            self.longest = self.longest.max(millis);
            return None;
        }
        let what = match seen {
            Seen::Measured => format!(
                "hlin-module: this module held the main thread for {} ms.",
                millis.round()
            ),
            Seen::Late => format!(
                "hlin-module: the main thread was held for about {} ms. This browser does not \
                 report long tasks, so this was seen from a late timer, and the page and other \
                 modules share the thread here: it may not have been this module.",
                millis.round()
            ),
        };
        let since = match self.held {
            0 => String::new(),
            held => format!(
                " Since the last warning, {held} more over {THRESHOLD_MILLIS} ms, the longest {} ms.",
                self.longest.round()
            ),
        };
        self.last_warned = Some(now);
        self.held = 0;
        self.longest = 0.0;
        Some(format!(
            "{what}{since} (A debug build warns from {THRESHOLD_MILLIS} ms, at most once every {} s.) {RULE}",
            QUIET_MILLIS / 1000.0
        ))
    }
}

/// Starts watching, for the life of the frame.
pub(crate) fn start() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let warner = Rc::new(RefCell::new(Warner::default()));
    if !observe_long_tasks(&warner) {
        watch_the_clock(&window, warner);
    }
}

fn warn(message: String) {
    web_sys::console::warn_1(&message.into());
}

/// Measures the frame's own long tasks, where the browser reports them.
/// `false` where it does not.
fn observe_long_tasks(warner: &Rc<RefCell<Warner>>) -> bool {
    let supported = Reflect::get(&js_sys::global(), &"PerformanceObserver".into())
        .and_then(|observer| Reflect::get(&observer, &"supportedEntryTypes".into()))
        .ok()
        .and_then(|types| types.dyn_into::<Array>().ok())
        .is_some_and(|types| types.includes(&"longtask".into(), 0));
    if !supported {
        return false;
    }
    let warner = warner.clone();
    let callback = Closure::<dyn FnMut(PerformanceObserverEntryList)>::new(
        move |entries: PerformanceObserverEntryList| {
            for entry in entries.get_entries() {
                let entry: PerformanceEntry = entry.unchecked_into();
                // `self`: this frame's own task. A frame sharing the page's
                // process also hears of the page's and other frames' long
                // tasks, which are not this module's doing.
                if entry.name() != "self" {
                    continue;
                }
                let ended = entry.start_time() + entry.duration();
                if let Some(message) =
                    warner
                        .borrow_mut()
                        .held(ended, entry.duration(), Seen::Measured)
                {
                    warn(message);
                }
            }
        },
    );
    let Ok(observer) = PerformanceObserver::new(callback.as_ref().unchecked_ref()) else {
        return false;
    };
    observer.observe(&PerformanceObserverInit::new(&Array::of1(
        &"longtask".into(),
    )));
    // The observer lives as long as the frame does.
    callback.forget();
    true
}

/// Notices a clock ticking late, where the browser reports no long tasks.
fn watch_the_clock(window: &Window, warner: Rc<RefCell<Warner>>) {
    let Some(performance) = window.performance() else {
        return;
    };
    let document = window.document();
    // The last tick, or `None` when there is nothing to measure from: at the
    // start, and while the document is hidden, when browsers slow timers down
    // on purpose.
    let last = Rc::new(Cell::new(None::<f64>));

    let ticked = last.clone();
    let tick = Closure::<dyn FnMut()>::new(move || {
        let now = performance.now();
        if document.as_ref().is_some_and(|document| document.hidden()) {
            ticked.set(None);
            return;
        }
        if let Some(before) = ticked.get() {
            let late = now - before - f64::from(TICK_MILLIS);
            if let Some(message) = warner.borrow_mut().held(now, late, Seen::Late) {
                warn(message);
            }
        }
        ticked.set(Some(now));
    });
    // A tab shown again after its timers were slowed has a late tick owed
    // that nobody's code caused.
    let shown = Closure::<dyn FnMut()>::new(move || last.set(None));
    if let Some(document) = window.document() {
        let _ = document
            .add_event_listener_with_callback("visibilitychange", shown.as_ref().unchecked_ref());
    }
    let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
        tick.as_ref().unchecked_ref(),
        TICK_MILLIS,
    );
    tick.forget();
    shown.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_task_is_not_warned_about() {
        let mut warner = Warner::default();
        assert_eq!(warner.held(1_000.0, 199.0, Seen::Measured), None);
        assert_eq!(warner.held(2_000.0, 50.0, Seen::Late), None);
    }

    #[test]
    fn a_long_task_is_warned_about_naming_its_length_and_the_rule() {
        let mut warner = Warner::default();
        let said = warner.held(1_000.0, 312.4, Seen::Measured).unwrap();
        assert!(
            said.starts_with("hlin-module: this module held the main thread for 312 ms."),
            "{said}"
        );
        assert!(said.ends_with(RULE), "{said}");
    }

    #[test]
    fn a_late_clock_says_it_may_not_have_been_this_module() {
        let mut warner = Warner::default();
        let said = warner.held(1_000.0, 260.0, Seen::Late).unwrap();
        assert!(
            said.starts_with("hlin-module: the main thread was held for about 260 ms."),
            "{said}"
        );
        assert!(said.contains("may not have been this module"), "{said}");
    }

    #[test]
    fn warnings_are_rate_limited_and_what_was_held_back_is_counted() {
        let mut warner = Warner::default();
        assert!(warner.held(1_000.0, 300.0, Seen::Measured).is_some());
        // A busy module: nothing more for ten seconds...
        assert_eq!(warner.held(2_000.0, 400.0, Seen::Measured), None);
        assert_eq!(warner.held(5_000.0, 900.0, Seen::Measured), None);
        assert_eq!(warner.held(10_999.0, 250.0, Seen::Measured), None);
        // ...then one line, carrying the count.
        let said = warner.held(11_000.0, 220.0, Seen::Measured).unwrap();
        assert!(
            said.contains("Since the last warning, 3 more over 200 ms, the longest 900 ms."),
            "{said}"
        );
        // And the count starts again.
        let said = warner.held(21_000.0, 220.0, Seen::Measured).unwrap();
        assert!(!said.contains("Since the last warning"), "{said}");
    }
}
