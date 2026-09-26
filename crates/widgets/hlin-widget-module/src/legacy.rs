//! The shape every widget's module had before [[HLIN-I-0013]]: a module with
//! its own UI, written against the SDK's `Module` rather than a client.
//!
//! Kept, unchanged, for the widgets not yet converted to components mounted
//! twice ([[HLIN-T-0096]], [[HLIN-T-0097]]). Nothing new is written against
//! it, and it goes when the last widget is converted.

use hlin_module::{Attempt, BridgeError, Module, Reply, Request};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;

use crate::words::{outcome, worth_retrying};
use crate::{KIT, Loaded, STYLE};

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
            // For `loaded_view`'s *Try again*, which is drawn wherever the
            // widget draws its data.
            provide_context(widget);
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
                let again = worth_retrying(&reply);
                loaded.set(match outcome(reply) {
                    Ok(answer) => match answer.json::<T>() {
                        Ok(value) => Loaded::Ready(value),
                        Err(_) => Loaded::Refused(
                            "The widget answered with something this module cannot read."
                                .to_string(),
                        ),
                    },
                    Err(words) if again => Loaded::Failed(words),
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

    /// Fetch everything [`Widget::load`]ed again, now.
    pub fn reload(&self) {
        self.reloads.update(|count| *count += 1);
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
/// `draw` with the answer. A read that could not reach the platform says so
/// and offers *Try again*.
pub fn loaded_view<T, V>(
    loaded: ReadSignal<Loaded<T>>,
    draw: impl Fn(T) -> V + Send + Sync + 'static,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    let widget = use_context::<Widget>();
    move || match loaded.get() {
        Loaded::Loading => view! { <p class="w-quiet">"Loading…"</p> }.into_any(),
        Loaded::Refused(words) => view! { <p class="w-refusal">{words}</p> }.into_any(),
        Loaded::Failed(words) => {
            let again = widget.map(|widget| {
                view! {
                    <button class="w-button" on:click=move |_| widget.reload()>
                        "Try again"
                    </button>
                }
            });
            view! {
                <div class="w-problem w-failed" role="status">
                    <p>{words}</p>
                    {again}
                </div>
            }
            .into_any()
        }
        Loaded::Ready(value) => draw(value).into_any(),
    }
}
