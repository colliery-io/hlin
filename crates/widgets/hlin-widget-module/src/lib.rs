//! The module side of the twenty widgets ([[HLIN-I-0012]]).
//!
//! `hlin-widget-support` is what a widget's server shares; this is what its
//! module shares. Each widget's module is an ordinary Leptos app on
//! `hlin-module`, and this takes away the part that would otherwise be copied
//! twenty times, taken from the checklist's and the feed's modules:
//!
//! - [`start`]: connect, wait for `init`, mount, say `ready`. With
//!   `wasm_bindgen_futures::spawn_local`, not Leptos's, whose executor does
//!   not exist until something is mounted.
//! - [`Widget::load`]: fetch from the widget's own platform now, and again
//!   whenever the shell's context changes, the shell relays a `changed` for
//!   this panel (the platform's stream, or another of its modules), or this
//!   module wrote something. Only the latest answer is shown.
//! - [`Widget::send`]: a write as an `Attempt`, so a network failure offers
//!   *Try again* with the same idempotency key; `changed` announced after one
//!   that happened; the data fetched again either way, so nothing is shown
//!   that the platform did not keep; and a refusal shown in the platform's
//!   words ([`platform_words`]), never the shell's developer-facing reason.
//! - [`STYLE`]: the one stylesheet every widget is drawn with, written against
//!   the shell's `--hlin-*` tokens only ([[HLIN-S-0007]], *theme*), each with
//!   a fallback for a token the shell does not send. Injected by [`start`] as
//!   an inline `<style>`, which the module CSP allows.
//!
//! A widget's own look goes in its `module/module.css`, against the same
//! tokens.

use hlin_module::hlin_bridge::Refusal;
use hlin_module::{Attempt, BridgeError, Module, Refused, Reply, Request};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// The stylesheet every widget is drawn with.
pub const STYLE: &str = include_str!("../widget.css");

/// What the shell logs as the kit a widget was drawn with, so drift between
/// widgets built against different versions of this crate can be seen.
pub const KIT: &str = concat!("hlin-widget@", env!("CARGO_PKG_VERSION"));

/// Connect to the shell, then mount `app` with this widget's handle.
///
/// `panel` is the panel key the widget's manifest declares; `changed` is
/// announced and listened for under it.
pub fn start<F, V>(panel: &'static str, app: F)
where
    F: FnOnce(Widget) -> V + 'static,
    V: IntoView + 'static,
{
    console_error_panic_hook::set_once();
    // Not Leptos's `spawn_local`: its executor starts when something is
    // mounted, and nothing is until `init` has arrived.
    wasm_bindgen_futures::spawn_local(async move {
        // Resolves when the shell's `init` arrives.
        let module = hlin_module::connect().await;
        let ready = module.clone();
        leptos::mount::mount_to_body(move || {
            let widget = Widget::new(module, panel);
            view! {
                <style>{STYLE}</style>
                <main class="widget">
                    {app(widget)}
                    <ProblemNotice widget />
                </main>
            }
        });
        ready.ready(Some(KIT));
    });
}

/// What a fetch has come back with.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded<T> {
    /// Nothing yet.
    Loading,
    /// The platform's answer.
    Ready(T),
    /// Why there is nothing to show, in words for a person.
    Refused(String),
}

impl<T> Loaded<T> {
    /// The answer, if there is one.
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A write that did not happen, and whatever is needed to try it again.
#[derive(Clone)]
struct Problem {
    text: String,
    /// The same attempt, with the same idempotency key, when the failure was
    /// the network's rather than a rule's. A rule would say no again.
    retry: Option<Attempt>,
}

/// A widget's handle on the shell: cheap to copy into any event handler.
#[derive(Clone, Copy)]
pub struct Widget {
    module: StoredValue<Module>,
    panel: &'static str,
    reloads: RwSignal<u64>,
    problem: RwSignal<Option<Problem>>,
}

impl Widget {
    fn new(module: Module, panel: &'static str) -> Self {
        Self {
            module: StoredValue::new(module),
            panel,
            reloads: RwSignal::new(0),
            problem: RwSignal::new(None),
        }
    }

    /// The SDK's handle, for anything this does not cover.
    pub fn module(&self) -> Module {
        self.module.get_value()
    }

    /// Whether the shell will refuse writes here, so the widget can stop
    /// offering them.
    pub fn read_only(&self) -> Signal<bool> {
        self.module.with_value(Module::read_only)
    }

    /// `request`'s answer as `T`, fetched now and again whenever something
    /// says it may have changed (see the crate docs).
    pub fn load<T>(&self, request: impl Fn() -> Request + 'static) -> ReadSignal<Loaded<T>>
    where
        T: DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let loaded = RwSignal::new(Loaded::Loading);
        let module = self.module;
        let context = module.with_value(Module::context);
        let changes = module.with_value(Module::changes);
        let panel = self.panel;
        let reloads = self.reloads;

        // A `changed` for this panel, by its sequence number, so two in a row
        // are still two reasons to look again.
        let changed_here = Memo::new(move |_| {
            changes.with(|change| {
                change
                    .as_ref()
                    .filter(|change| change.panel == panel)
                    .map(|change| change.seq)
            })
        });

        let latest = StoredValue::new(0_u64);
        Effect::new(move |_| {
            context.track();
            // Read, not tracked: a memo that is only tracked is never
            // computed, and so never learns that anything it depends on
            // changed.
            let _ = changed_here.get();
            reloads.track();
            latest.update_value(|count| *count += 1);
            let mine = latest.get_value();
            let asked = request();
            spawn_local(async move {
                let reply = module.get_value().fetch(asked).await;
                if latest.get_value() != mine {
                    return;
                }
                loaded.set(match outcome(reply) {
                    Ok(answer) => match answer.json::<T>() {
                        Ok(value) => Loaded::Ready(value),
                        Err(_) => Loaded::Refused(
                            "The widget answered with something this module cannot read."
                                .to_string(),
                        ),
                    },
                    Err(words) => Loaded::Refused(words),
                });
            });
        });

        loaded.read_only()
    }

    /// Send a write. Whatever happens, everything [`Widget::load`]ed is
    /// fetched again; if it happened, the shell is told, so the platform's
    /// other modules on the page fetch again too.
    pub fn send(&self, request: Request) {
        self.send_then(request, |_| {});
    }

    /// [`Widget::send`], then `done` with whether it happened.
    pub fn send_then(&self, request: Request, done: impl FnOnce(bool) + 'static) {
        let attempt = self.module.with_value(|module| module.attempt(request));
        let widget = *self;
        spawn_local(async move {
            let reply = attempt.send().await;
            done(widget.settle(reply, attempt));
        });
    }

    fn retry(self, attempt: Attempt) {
        spawn_local(async move {
            let reply = attempt.retry().await;
            self.settle(reply, attempt);
        });
    }

    fn settle(self, reply: Result<Reply, BridgeError>, attempt: Attempt) -> bool {
        let retry = worth_retrying(&reply);
        let happened = match outcome(reply) {
            Ok(_) => {
                self.problem.set(None);
                // The shell never infers a change from a write; the module
                // says so, and the shell tells the platform's other modules.
                self.module
                    .with_value(|module| module.changed(self.panel, Default::default()));
                true
            }
            Err(text) => {
                self.problem.set(Some(Problem {
                    text,
                    retry: retry.then_some(attempt),
                }));
                false
            }
        };
        self.reloads.update(|count| *count += 1);
        happened
    }
}

/// A write that did not happen, in the words of whoever said no.
#[component]
fn ProblemNotice(widget: Widget) -> impl IntoView {
    move || {
        widget.problem.get().map(|problem| {
            let retry = problem.retry.clone().map(|attempt| {
                view! {
                    <button class="w-button" on:click=move |_| widget.retry(attempt.clone())>
                        "Try again"
                    </button>
                }
            });
            view! {
                <div class="w-problem" role="status">
                    <p>{problem.text}</p>
                    {retry}
                    <button class="w-button w-button--quiet" on:click=move |_| widget.problem.set(None)>
                        "Dismiss"
                    </button>
                </div>
            }
        })
    }
}

/// Draw `loaded`: a quiet line while loading, the refusal in its words, or
/// `draw` with the answer.
pub fn loaded_view<T, V>(
    loaded: ReadSignal<Loaded<T>>,
    draw: impl Fn(T) -> V + Send + Sync + 'static,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    move || match loaded.get() {
        Loaded::Loading => view! { <p class="w-quiet">"Loading…"</p> }.into_any(),
        Loaded::Refused(words) => view! { <p class="w-refusal">{words}</p> }.into_any(),
        Loaded::Ready(value) => draw(value).into_any(),
    }
}

// -- Words -------------------------------------------------------------------

/// A successful answer, or what to tell the person.
pub fn outcome(reply: Result<Reply, BridgeError>) -> Result<hlin_module::Answer, String> {
    match reply {
        Ok(Reply::Answered(answer)) if answer.is_success() => Ok(answer),
        Ok(Reply::Answered(answer)) => Err(platform_words(&answer)),
        Ok(Reply::Refused(refused)) => Err(shell_words(&refused)),
        Err(_) => Err("The panel closed before the widget answered.".to_string()),
    }
}

/// The platform's own words for a refusal, shown exactly as it wrote them.
///
/// Every widget answers a refusal it decides with `{"message": ...}`, written
/// for the person who clicked. Anything else gets a plain sentence with the
/// status in it.
pub fn platform_words(answer: &hlin_module::Answer) -> String {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match answer.json::<Message>() {
        Ok(said) if !said.message.trim().is_empty() => said.message,
        _ => format!("The widget said no (status {}).", answer.status),
    }
}

/// What to say when the shell, not the platform, refused. The shell's own
/// `reason` is for a developer, so it is not shown.
pub fn shell_words(refused: &Refused) -> String {
    match refused.refusal {
        Refusal::ReadOnly => "This surface is read-only, so nothing is sent.".to_string(),
        Refusal::NotSignedIn => "You have been signed out. Sign in again to carry on.".to_string(),
        Refusal::Unreachable | Refusal::Timeout => {
            "The widget could not be reached. Try again in a moment.".to_string()
        }
        Refusal::TooMany => "Too much at once. Try again in a moment.".to_string(),
        Refusal::TooLarge => "That is too much to send.".to_string(),
        _ => "Hlin did not send this to the widget.".to_string(),
    }
}

/// Whether trying the same write again might work: the network, not a rule.
pub fn worth_retrying(reply: &Result<Reply, BridgeError>) -> bool {
    match reply {
        Ok(Reply::Refused(refused)) => matches!(
            refused.refusal,
            Refusal::Unreachable | Refusal::Timeout | Refusal::TooMany
        ),
        Ok(Reply::Answered(answer)) => answer.status >= 500,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn answer(status: u16, body: &str) -> hlin_module::Answer {
        hlin_module::Answer {
            status,
            headers: BTreeMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_refusal_is_shown_in_the_widgets_own_words() {
        assert_eq!(
            platform_words(&answer(
                409,
                r#"{"message":"The counter is already at zero"}"#
            )),
            "The counter is already at zero"
        );
    }

    #[test]
    fn an_answer_without_words_still_says_what_happened() {
        assert_eq!(
            platform_words(&answer(500, "<html>")),
            "The widget said no (status 500)."
        );
    }

    #[test]
    fn the_shells_reason_is_never_shown_as_if_the_widget_said_it() {
        let refused = Refused {
            refusal: Refusal::OutsidePrefix,
            status: 403,
            reason: "internal detail".to_string(),
        };
        assert!(!shell_words(&refused).contains("internal detail"));
        assert!(!worth_retrying(&Ok(Reply::Refused(refused))));
    }

    #[test]
    fn only_the_networks_failures_are_worth_retrying() {
        let unreachable = Refused {
            refusal: Refusal::Unreachable,
            status: 502,
            reason: String::new(),
        };
        assert!(worth_retrying(&Ok(Reply::Refused(unreachable))));
        assert!(worth_retrying(&Ok(Reply::Answered(answer(503, "")))));
        assert!(!worth_retrying(&Ok(Reply::Answered(answer(409, "")))));
    }

    #[test]
    fn the_kit_names_this_crate_and_its_version() {
        assert!(KIT.starts_with("hlin-widget@"));
    }
}
