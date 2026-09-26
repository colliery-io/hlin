//! The handle a widget's components use, written once over [`Client`].
//!
//! - [`Widget::load`]: fetch now, and again whenever the client says what was
//!   loaded may be out of date, or this widget wrote something. Only the
//!   latest answer is shown. A read that could not reach the platform offers
//!   *Try again*, and is tried again by itself on the next change, which both
//!   clients also report when the platform's event stream comes back.
//! - [`Widget::send`]: a write as an [`Attempt`], so a network failure offers
//!   *Try again* with the same idempotency key; announced after one that
//!   happened; the data fetched again either way, so nothing is shown that the
//!   platform did not keep; and a refusal shown in the platform's words
//!   ([`platform_words`]).

use std::rc::Rc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::client::{Answer, Attempt, Client, Reply, Request, Streamed, TimeRange, Trouble};

/// The stylesheet every widget is drawn with.
pub const STYLE: &str = include_str!("../widget.css");

/// Mount `app` with `client` as its widget's transport: the handle in
/// context, the shared stylesheet, and the notice a failed write leaves.
///
/// What each transport's own `mount` calls once it has a client; a widget
/// calls one of those, not this.
pub fn mount<C, F, V>(client: C, app: F)
where
    C: Client,
    F: FnOnce() -> V + 'static,
    V: IntoView + 'static,
{
    leptos::mount::mount_to_body(move || {
        let widget = Widget::new(Rc::new(client));
        provide_context(widget);
        view! {
            <style>{STYLE}</style>
            <main class="widget">
                {app()}
                <ProblemNotice widget />
            </main>
        }
    });
}

/// The widget's handle, from context: what every component starts with.
///
/// # Panics
///
/// Outside a tree mounted by a transport's `mount`, where there is no client
/// to use.
pub fn use_widget() -> Widget {
    use_context::<Widget>()
        .expect("a widget's components are mounted by hlin_widget_module or hlin_widget_ui::direct")
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
    /// The platform could not be reached, in words for a person. Trying
    /// again may work: [`loaded_view`] offers it, and the next change does it
    /// anyway.
    Failed(String),
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
    retry: Option<Rc<dyn Attempt>>,
}

/// A widget's handle on its client: cheap to copy into any event handler.
#[derive(Clone, Copy)]
pub struct Widget {
    client: StoredValue<Rc<dyn Client>, LocalStorage>,
    reloads: RwSignal<u64>,
    problem: RwSignal<Option<Problem>, LocalStorage>,
}

impl Widget {
    fn new(client: Rc<dyn Client>) -> Self {
        Self {
            client: StoredValue::new_local(client),
            reloads: RwSignal::new(0),
            problem: RwSignal::new_local(None),
        }
    }

    /// Whether writes will be refused, so the widget can stop offering them.
    pub fn read_only(&self) -> Signal<bool> {
        self.client.with_value(|client| client.read_only())
    }

    /// Whether anybody can see the widget.
    pub fn visible(&self) -> Signal<bool> {
        self.client.with_value(|client| client.visible())
    }

    /// The time range the widget is asked to show, where there is one.
    pub fn time_range(&self) -> Signal<Option<TimeRange>> {
        self.client.with_value(|client| client.time_range())
    }

    /// What the widget kept when it was last suspended, if it is being
    /// brought back.
    pub fn restored(&self) -> Option<Vec<u8>> {
        self.client.with_value(|client| client.restored())
    }

    /// What to keep if the widget is suspended.
    pub fn on_suspend(&self, keep: impl Fn() -> Option<Vec<u8>> + 'static) {
        self.client
            .with_value(|client| client.on_suspend(Box::new(keep)));
    }

    /// A read followed as it arrives (see [`Client::stream`]).
    pub async fn stream(&self, request: Request) -> Streamed {
        let pending = self.client.with_value(|client| client.stream(request));
        pending.await
    }

    /// `request`'s answer as `T`, fetched now and again whenever something
    /// says it may have changed (see the module docs).
    pub fn load<T>(&self, request: impl Fn() -> Request + 'static) -> ReadSignal<Loaded<T>>
    where
        T: DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let loaded = RwSignal::new(Loaded::Loading);
        let client = self.client;
        let changes = client.with_value(|client| client.changes());
        let reloads = self.reloads;

        let latest = StoredValue::new(0_u64);
        Effect::new(move |_| {
            changes.track();
            reloads.track();
            latest.update_value(|count| *count += 1);
            let mine = latest.get_value();
            let pending = client.with_value(|client| client.fetch(request()));
            spawn_local(async move {
                let reply = pending.await;
                if latest.get_value() != mine {
                    return;
                }
                loaded.set(match outcome(reply) {
                    Ok(answer) => match answer.json::<T>() {
                        Ok(value) => Loaded::Ready(value),
                        Err(_) => Loaded::Refused(
                            "The widget answered with something this page cannot read.".to_string(),
                        ),
                    },
                    Err(trouble) if trouble.retry => Loaded::Failed(trouble.words),
                    Err(trouble) => Loaded::Refused(trouble.words),
                });
            });
        });

        loaded.read_only()
    }

    /// Send a write. Whatever happens, everything [`Widget::load`]ed is
    /// fetched again; if it happened, it is announced.
    pub fn send(&self, request: Request) {
        self.send_then(request, |_| {});
    }

    /// [`Widget::send`], then `done` with whether it happened.
    pub fn send_then(&self, request: Request, done: impl FnOnce(bool) + 'static) {
        let attempt: Rc<dyn Attempt> =
            Rc::from(self.client.with_value(|client| client.attempt(request)));
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

    fn retry(self, attempt: Rc<dyn Attempt>) {
        spawn_local(async move {
            let reply = attempt.send().await;
            self.settle(reply, attempt);
        });
    }

    fn settle(self, reply: Reply, attempt: Rc<dyn Attempt>) -> bool {
        let happened = match outcome(reply) {
            Ok(_) => {
                self.problem.set(None);
                self.client.with_value(|client| client.announce());
                true
            }
            Err(trouble) => {
                self.problem.set(Some(Problem {
                    text: trouble.words,
                    retry: trouble.retry.then_some(attempt),
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

// -- Words -------------------------------------------------------------------

/// A successful answer, or what to tell the person and whether trying again
/// might help.
pub fn outcome(reply: Reply) -> Result<Answer, Trouble> {
    match reply {
        Ok(answer) if answer.is_success() => Ok(answer),
        Ok(answer) => Err(Trouble {
            words: platform_words(&answer),
            retry: answer.status >= 500,
        }),
        Err(trouble) => Err(trouble),
    }
}

/// The platform's own words for a refusal, shown exactly as it wrote them.
///
/// Every widget answers a refusal it decides with `{"message": ...}`, written
/// for the person who clicked. Anything else gets a plain sentence with the
/// status in it.
pub fn platform_words(answer: &Answer) -> String {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match answer.json::<Message>() {
        Ok(said) if !said.message.trim().is_empty() => said.message,
        _ => format!("The widget said no (status {}).", answer.status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: u16, body: &str) -> Answer {
        Answer {
            status,
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
    fn only_a_platform_that_failed_rather_than_refused_is_worth_trying_again() {
        assert!(outcome(Ok(answer(503, ""))).unwrap_err().retry);
        assert!(!outcome(Ok(answer(409, "")).clone()).unwrap_err().retry);
        assert!(outcome(Ok(answer(204, ""))).is_ok());
        let unreachable = Trouble {
            words: "gone".to_string(),
            retry: true,
        };
        assert_eq!(outcome(Err(unreachable.clone())), Err(unreachable));
    }
}
