//! What the module draws, and when it asks the feed again.
//!
//! The same shape as the checklist's module, so either can be copied:
//!
//! - **What is shown comes from the platform, every time.** A write is sent,
//!   and whatever the answer the posts are fetched again, so the view never
//!   shows a post the feed did not keep.
//! - **Refetch when something says the posts changed**: the shell's `context`,
//!   its `changed` (the feed's event stream, or another of its modules), and
//!   this module's own writes.
//! - **Refusals are the feed's.** The feed tells its module nothing in advance
//!   about who may do what, so edit and delete are offered on every post, and
//!   when the feed says no, its words are shown as they came.

use aurora_leptos::{Alert, Button, COMPONENTS_CSS, Empty, Loading, TOKENS_CSS, Textarea};
use hlin_module::{Attempt, Module, Request};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, PANEL, Post};

/// What the posts area shows.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    /// Nothing has come back yet.
    Loading,
    /// The posts, newest first, as the feed last described them.
    Posts(Vec<Post>),
    /// The feed would not show them, in its words.
    Refused(String),
}

/// A write that did not happen, and whatever is needed to try it again.
#[derive(Clone)]
struct Problem {
    text: String,
    /// The same attempt, with the same idempotency key, when the failure was
    /// the network's rather than a rule's. A rule would say no again.
    retry: Option<Attempt>,
}

/// Everything a write needs, cheap to copy into any event handler.
#[derive(Clone, Copy)]
struct Writer {
    module: StoredValue<Module>,
    problem: RwSignal<Option<Problem>>,
    reloads: RwSignal<u64>,
}

impl Writer {
    /// Sends a write, then calls `done` with whether it happened.
    fn send(self, request: Request, done: impl FnOnce(bool) + 'static) {
        let attempt = self.module.with_value(|module| module.attempt(request));
        spawn_local(async move {
            let reply = attempt.send().await;
            done(self.settle(reply, attempt));
        });
    }

    /// Sends the failed write again, with the key it was first sent with, so
    /// the feed can tell a retry from a second post.
    fn retry(self, attempt: Attempt) {
        spawn_local(async move {
            let reply = attempt.retry().await;
            self.settle(reply, attempt);
        });
    }

    /// What follows any write: announce it if it happened, show why if not,
    /// and fetch the posts again either way.
    fn settle(
        self,
        reply: Result<hlin_module::Reply, hlin_module::BridgeError>,
        attempt: Attempt,
    ) -> bool {
        let retry = api::worth_retrying(&reply);
        let happened = match api::outcome(reply) {
            Ok(_) => {
                self.problem.set(None);
                // The shell never infers a change from a write; the module
                // says so, and the shell tells the feed's other modules.
                self.module
                    .with_value(|module| module.changed(PANEL, Default::default()));
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

/// The feed.
#[component]
pub fn Feed(module: Module) -> impl IntoView {
    let context = module.context();
    let changes = module.changes();
    let read_only = module.read_only();
    let module = StoredValue::new(module);

    let shown = RwSignal::new(Shown::Loading);
    let problem = RwSignal::new(None::<Problem>);
    let reloads = RwSignal::new(0_u64);
    let now = RwSignal::new(0_i64);
    let writer = Writer {
        module,
        problem,
        reloads,
    };

    // A `changed` that concerns the posts, by its sequence number, so two in a
    // row are still two reasons to look again.
    let changed_here = Memo::new(move |_| {
        changes.with(|change| {
            change
                .as_ref()
                .filter(|change| change.panel == PANEL)
                .map(|change| change.seq)
        })
    });

    // Fetch the posts whenever anything above says to. Only the latest fetch's
    // answer is shown, so a slow answer never replaces a newer one.
    let latest = StoredValue::new(0_u64);
    Effect::new(move |_| {
        context.track();
        // Read, not tracked: a memo that is only tracked is never computed,
        // and so never learns that anything it depends on changed.
        let _ = changed_here.get();
        reloads.track();
        latest.update_value(|count| *count += 1);
        let mine = latest.get_value();
        spawn_local(async move {
            let answer = api::send(&module.get_value(), api::posts()).await;
            if latest.get_value() != mine {
                return;
            }
            now.set(js_sys::Date::now() as i64);
            shown.set(match answer.and_then(|answer| api::read_posts(&answer)) {
                Ok(posts) => Shown::Posts(posts),
                Err(words) => Shown::Refused(words),
            });
        });
    });

    // Redrawn only when what kind of thing is shown changes; fresh posts reach
    // the keyed rows through `posts`, so a post being edited here survives
    // somebody else posting.
    let posts = Memo::new(move |_| match shown.get() {
        Shown::Posts(posts) => Some(posts),
        _ => None,
    });
    let showing_kind = Memo::new(move |_| std::mem::discriminant(&shown.get()));
    let writes = Signal::derive(move || !read_only.get());

    view! {
        <style>{TOKENS_CSS}{COMPONENTS_CSS}</style>
        <div class="feed">
            {move || writes.get().then(|| view! { <Compose writer /> })}
            {move || problem.get().map(|problem| view! { <ProblemNotice problem writer /> })}
            {move || {
                let _ = showing_kind.get();
                match shown.get_untracked() {
                    Shown::Loading => view! { <Loading label="Loading posts…" /> }.into_any(),
                    Shown::Refused(_) => {
                        let words = move || match shown.get() {
                            Shown::Refused(words) => words,
                            _ => String::new(),
                        };
                        view! {
                            <Alert title="The feed says">
                                <p class="refusal">{words}</p>
                            </Alert>
                        }
                        .into_any()
                    }
                    Shown::Posts(_) => view! { <Posts posts now writer writes /> }.into_any(),
                }
            }}
        </div>
    }
}

/// A write that did not happen, in the words of whoever said no.
#[component]
fn ProblemNotice(problem: Problem, writer: Writer) -> impl IntoView {
    let retry = problem.retry.clone().map(|attempt| {
        let again = Callback::new(move |_| writer.retry(attempt.clone()));
        view! { <Button variant="light" size="xs" on_click=again>"Try again"</Button> }
    });
    let dismiss = Callback::new(move |_| writer.problem.set(None));
    view! {
        <Alert>
            <p class="refusal" role="status">{problem.text}</p>
            {retry}
            <Button variant="subtle" size="xs" on_click=dismiss>"Dismiss"</Button>
        </Alert>
    }
}

/// The box a new post is written in.
#[component]
fn Compose(writer: Writer) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let submit = move |event: leptos::ev::SubmitEvent| {
        // `form-action 'none'` would stop the browser going anywhere; this
        // stops it trying.
        event.prevent_default();
        let body = text.get_untracked().trim().to_string();
        if body.is_empty() {
            return;
        }
        writer.send(api::create(&body), move |happened| {
            if happened {
                text.set(String::new());
            }
        });
    };
    view! {
        <form class="compose" on:submit=submit>
            <Textarea placeholder="Say something to everyone" value=text rows=2 />
            <div class="compose__actions">
                <Button>"Post"</Button>
            </div>
        </form>
    }
}

/// Every post, newest first.
#[component]
fn Posts(
    posts: Memo<Option<Vec<Post>>>,
    now: RwSignal<i64>,
    writer: Writer,
    writes: Signal<bool>,
) -> impl IntoView {
    let empty = move || posts.with(|posts| posts.as_ref().is_none_or(Vec::is_empty));
    view! {
        {move || empty().then(|| view! { <Empty message="Nobody has posted yet." /> })}
        <ul class="posts">
            <For
                each=move || posts.get().unwrap_or_default()
                key=|post| post.clone()
                children=move |post| view! { <PostRow post now writer writes /> }
            />
        </ul>
    }
}

/// One post, with edit and delete.
#[component]
fn PostRow(post: Post, now: RwSignal<i64>, writer: Writer, writes: Signal<bool>) -> impl IntoView {
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(post.body.clone());

    let save = {
        let (id, before) = (post.id.clone(), post.body.clone());
        Callback::new(move |_| {
            let body = draft.get_untracked().trim().to_string();
            if body.is_empty() || body == before {
                editing.set(false);
                return;
            }
            writer.send(api::edit(&id, &body), move |happened| {
                if happened {
                    editing.set(false);
                }
            });
        })
    };
    let delete = {
        let id = post.id.clone();
        Callback::new(move |_| writer.send(api::delete(&id), |_| {}))
    };
    let start_editing = {
        let body = post.body.clone();
        Callback::new(move |_| {
            draft.set(body.clone());
            editing.set(true);
        })
    };
    let cancel = Callback::new(move |_| editing.set(false));

    let posted_at = post.posted_at.clone();
    let when = move || api::ago(&posted_at, now.get());
    let edited = post.edited_at.is_some().then_some(" · edited");
    let body = post.body.clone();

    view! {
        <li class="post" data-post=post.id.clone()>
            <div class="post__head">
                <span class="post__author">{post.author.name.clone()}</span>
                <time class="post__time" datetime=post.posted_at.clone() title=post.posted_at.clone()>
                    {when}
                    {edited}
                </time>
            </div>
            {move || {
                if editing.get() {
                    view! {
                        <Textarea value=draft rows=2 />
                        <div class="post__actions">
                            <Button size="xs" on_click=save>"Save"</Button>
                            <Button variant="subtle" size="xs" on_click=cancel>"Cancel"</Button>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <p class="post__body">{body.clone()}</p>
                        {writes.get().then(|| view! {
                            <div class="post__actions">
                                <Button variant="subtle" size="xs" on_click=start_editing>"Edit"</Button>
                                <Button variant="subtle" size="xs" bad=true on_click=delete>"Delete"</Button>
                            </div>
                        })}
                    }
                    .into_any()
                }
            }}
        </li>
    }
}
