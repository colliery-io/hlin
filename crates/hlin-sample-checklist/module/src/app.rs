//! What the module draws, and when it asks the platform again.
//!
//! The shape is the one a platform team should copy:
//!
//! - **What is shown comes from the platform, every time.** Nothing is changed
//!   on screen ahead of the platform's answer. A write is sent, and whatever
//!   the answer, the list is fetched again, so the view never shows a change
//!   the platform did not make.
//! - **Refetch when something says the list changed**: the shell's `context`
//!   (a different list chosen), its `changed` (the platform's event stream, or
//!   another of the platform's modules), and this module's own writes.
//! - **Hide what the platform would refuse, and show its refusal anyway.** The
//!   list tells its own module who the viewer is to it, so edit and delete are
//!   offered only to an item's author and the list's owner. The platform
//!   decides again regardless, and if it says no, its words are shown as they
//!   came.

use aurora_leptos::{Alert, Button, COMPONENTS_CSS, Empty, Loading, TOKENS_CSS, TextInput};
use hlin_module::hlin_bridge::Selections;
use hlin_module::{Attempt, Module, Request};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Item, LIST_PARAM, ListPage, ListSummary, PANEL};

/// What the list area shows.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    /// Nothing has come back yet.
    Loading,
    /// The viewer is on no list at all.
    NoLists,
    /// A list, as the platform last described it.
    Page(ListPage),
    /// The platform would not show this list, in its words.
    Refused(String),
}

/// A write that did not happen, and whatever is needed to try it again.
#[derive(Clone)]
struct Problem {
    text: String,
    /// The same attempt, with the same idempotency key, when the failure was
    /// the network's rather than a rule's. A rule would say no again.
    retry: Option<(Attempt, String)>,
}

/// Everything a write needs, cheap to copy into any event handler.
#[derive(Clone, Copy)]
struct Writer {
    module: StoredValue<Module>,
    problem: RwSignal<Option<Problem>>,
    reloads: RwSignal<u64>,
}

impl Writer {
    /// Sends a write to `list`, then calls `done` with whether it happened.
    fn send(self, list: String, request: Request, done: impl FnOnce(bool) + 'static) {
        let attempt = self.module.with_value(|module| module.attempt(request));
        spawn_local(async move {
            let reply = attempt.send().await;
            done(self.settle(reply, attempt, list));
        });
    }

    /// Sends the failed write again, with the key it was first sent with, so
    /// the platform can tell a retry from a second change.
    fn retry(self, attempt: Attempt, list: String) {
        spawn_local(async move {
            let reply = attempt.retry().await;
            self.settle(reply, attempt, list);
        });
    }

    /// What follows any write: announce it if it happened, show why if not,
    /// and fetch the list again either way.
    fn settle(
        self,
        reply: Result<hlin_module::Reply, hlin_module::BridgeError>,
        attempt: Attempt,
        list: String,
    ) -> bool {
        let retry = api::worth_retrying(&reply);
        let happened = match api::outcome(reply) {
            Ok(_) => {
                self.problem.set(None);
                // The shell never infers a change from a write; the module
                // says so, and the shell tells the platform's other modules.
                let mut selections = Selections::new();
                selections.insert(LIST_PARAM.to_string(), vec![list]);
                self.module
                    .with_value(|module| module.changed(PANEL, selections));
                true
            }
            Err(text) => {
                self.problem.set(Some(Problem {
                    text,
                    retry: retry.then_some((attempt, list)),
                }));
                false
            }
        };
        self.reloads.update(|count| *count += 1);
        happened
    }
}

/// The checklist, for whichever list is chosen.
#[component]
pub fn Checklist(module: Module) -> impl IntoView {
    let context = module.context();
    let changes = module.changes();
    let read_only = module.read_only();
    let module = StoredValue::new(module);

    let lists = RwSignal::new(None::<Vec<ListSummary>>);
    let shown = RwSignal::new(Shown::Loading);
    let problem = RwSignal::new(None::<Problem>);
    let reloads = RwSignal::new(0_u64);
    let writer = Writer {
        module,
        problem,
        reloads,
    };

    // A list picked here, until the shell's `context` says which list is
    // chosen. `set-param` asks the shell to choose it, and the shell answers
    // with a `context`; until it does, or on a shell that ignores the request,
    // this keeps the picker doing what the person asked.
    let picked = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        context.track();
        picked.set(None);
    });

    // The one picked here, until the shell's next `context`; else the
    // shell's choice; else the viewer's first list, as the platform's own
    // table does with nothing chosen.
    let chosen = Memo::new(move |_| {
        picked.get().or_else(|| {
            context
                .with(|context| context.params.get(LIST_PARAM)?.first().cloned())
                .or_else(|| lists.with(|lists| Some(lists.as_ref()?.first()?.id.clone())))
        })
    });

    // The lists the viewer is on, for the picker. Asked for again when the
    // context changes, which is also when a person may have joined one.
    Effect::new(move |_| {
        context.track();
        spawn_local(async move {
            let request = api::lists();
            let answer = api::send(&module.get_value(), request).await;
            match answer.and_then(|answer| api::read_lists(&answer)) {
                Ok(found) => {
                    if found.is_empty() {
                        shown.set(Shown::NoLists);
                    }
                    lists.set(Some(found));
                }
                Err(words) => shown.set(Shown::Refused(words)),
            }
        });
    });

    // A `changed` that concerns this panel. Its sequence number, so two
    // identical changes in a row are still two reasons to look again.
    let changed_here = Memo::new(move |_| {
        changes.with(|change| {
            change
                .as_ref()
                .filter(|change| change.panel == PANEL)
                .filter(|change| {
                    // A change names the list it was made on; one naming none
                    // is about every list.
                    change.selections.get(LIST_PARAM).is_none_or(|lists| {
                        chosen.with_untracked(|chosen| {
                            chosen.as_ref().is_none_or(|chosen| lists.contains(chosen))
                        })
                    })
                })
                .map(|change| change.seq)
        })
    });

    // Fetch the chosen list whenever any of the above says to. Only the
    // latest fetch's answer is shown, so a slow answer for a list the person
    // has since left never replaces the one they are looking at.
    let latest = StoredValue::new(0_u64);
    let showing = StoredValue::new(None::<String>);
    Effect::new(move |_| {
        // Read, not tracked: a memo that is only tracked is never computed,
        // and so never learns that anything it depends on changed.
        let _ = changed_here.get();
        reloads.track();
        let Some(list) = chosen.get() else {
            return;
        };
        if showing.get_value().as_ref() != Some(&list) {
            // A different list: say so, rather than showing the last one's
            // items under the new one's name.
            shown.set(Shown::Loading);
            showing.set_value(Some(list.clone()));
        }
        latest.update_value(|count| *count += 1);
        let mine = latest.get_value();
        spawn_local(async move {
            let answer = api::send(&module.get_value(), api::list(&list)).await;
            if latest.get_value() != mine {
                return;
            }
            shown.set(match answer.and_then(|answer| api::read_list(&answer)) {
                Ok(page) => Shown::Page(page),
                Err(words) => Shown::Refused(words),
            });
        });
    });

    let chosen_is_offered = Memo::new(move |_| {
        let chosen = chosen.get();
        // Until the lists arrive there is nothing to say either way.
        lists.with(|lists| {
            lists
                .as_ref()
                .is_none_or(|lists| lists.iter().any(|list| Some(&list.id) == chosen.as_ref()))
        })
    });

    let pick = move |event: leptos::ev::Event| {
        let list = event_target_value(&event);
        picked.set(Some(list.clone()));
        module.with_value(|module| module.set_param(LIST_PARAM, vec![list]));
    };

    // The list area is redrawn only when what kind of thing it shows changes.
    // A fresh answer for the same list reaches the rows through `page`, and the
    // rows are keyed, so an item somebody else changed is redrawn and an item
    // being edited here is left alone.
    let page = Memo::new(move |_| match shown.get() {
        Shown::Page(page) => Some(page),
        _ => None,
    });
    let showing_kind = Memo::new(move |_| std::mem::discriminant(&shown.get()));

    view! {
        <style>{TOKENS_CSS}{COMPONENTS_CSS}</style>
        <div class="checklist">
            <label class="checklist__head">
                <span class="checklist__label">"List"</span>
                <select class="cl-input cl-select checklist__picker" on:change=pick>
                    // The surface may have chosen a list this viewer is not
                    // on. Say so, rather than showing one of theirs as if it
                    // were the one below.
                    {move || {
                        (!chosen_is_offered.get())
                            .then(|| view! { <option value="" selected disabled>"Pick a list"</option> })
                    }}
                    <For
                        each=move || lists.get().unwrap_or_default()
                        key=|list| list.id.clone()
                        children=move |list| {
                            let id = list.id.clone();
                            view! {
                                <option
                                    value=list.id.clone()
                                    prop:selected=move || chosen.get().as_ref() == Some(&id)
                                >
                                    {list.name}
                                </option>
                            }
                        }
                    />
                </select>
            </label>
            {move || problem.get().map(|problem| view! { <ProblemNotice problem writer /> })}
            {move || {
                let _ = showing_kind.get();
                match shown.get_untracked() {
                    Shown::Loading => view! { <Loading label="Loading the list…" /> }.into_any(),
                    Shown::NoLists => {
                        view! { <Empty message="You are not on any list yet." /> }.into_any()
                    }
                    Shown::Refused(_) => {
                        let words = move || match shown.get() {
                            Shown::Refused(words) => words,
                            _ => String::new(),
                        };
                        view! {
                            <Alert title="The checklist says">
                                <p class="refusal">{words}</p>
                            </Alert>
                        }
                        .into_any()
                    }
                    Shown::Page(_) => view! {
                        <Items page writer writes=Signal::derive(move || !read_only.get()) />
                    }
                    .into_any(),
                }
            }}
        </div>
    }
}

/// A write that did not happen, in the words of whoever said no.
#[component]
fn ProblemNotice(problem: Problem, writer: Writer) -> impl IntoView {
    let retry = problem.retry.clone().map(|(attempt, list)| {
        let again = Callback::new(move |_| writer.retry(attempt.clone(), list.clone()));
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

/// One item as a row shows it: which list it is on, and whether the viewer
/// may change it. The whole of it is the row's key, so a row is redrawn
/// exactly when any of it changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Line {
    list: String,
    item: Item,
    may_change: bool,
}

/// One list's items and the field to add another.
#[component]
fn Items(page: Memo<Option<ListPage>>, writer: Writer, writes: Signal<bool>) -> impl IntoView {
    let lines = move || {
        page.with(|page| {
            page.as_ref()
                .map(|page| {
                    page.items
                        .iter()
                        .map(|item| Line {
                            list: page.list.id.clone(),
                            item: item.clone(),
                            may_change: page.may_change(item),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let empty = move || page.with(|page| page.as_ref().is_none_or(|page| page.items.is_empty()));
    let list = Memo::new(move |_| {
        page.with(|page| page.as_ref().map(|page| page.list.id.clone()))
            .unwrap_or_default()
    });

    view! {
        {move || empty().then(|| view! { <p class="quiet">"Nothing on this list yet."</p> })}
        <ul class="checklist__items">
            <For
                each=lines
                key=|line| line.clone()
                children=move |line| view! { <Row line writer writes /> }
            />
        </ul>
        {move || writes.get().then(|| view! { <AddItem list writer /> })}
    }
}

/// One item: cross it off, and edit or delete it if it is yours to change.
#[component]
fn Row(line: Line, writer: Writer, writes: Signal<bool>) -> impl IntoView {
    let Line {
        list,
        item,
        may_change,
    } = line;
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(item.text.clone());
    let busy = RwSignal::new(false);

    let toggle = {
        let (list, id) = (list.clone(), item.id.clone());
        move |_| {
            busy.set(true);
            writer.send(list.clone(), api::toggle(&list, &id), move |_| {
                busy.set(false)
            });
        }
    };
    let save = {
        let (list, id, before) = (list.clone(), item.id.clone(), item.text.clone());
        move || {
            let text = draft.get_untracked().trim().to_string();
            if text.is_empty() || text == before {
                editing.set(false);
                return;
            }
            writer.send(
                list.clone(),
                api::edit(&list, &id, &text),
                move |happened| {
                    if happened {
                        editing.set(false);
                    }
                },
            );
        }
    };
    let delete = {
        let (list, id) = (list.clone(), item.id.clone());
        Callback::new(move |_| writer.send(list.clone(), api::delete(&list, &id), |_| {}))
    };
    let save_on_key = {
        let save = save.clone();
        move |event: leptos::ev::KeyboardEvent| match event.key().as_str() {
            "Enter" => save(),
            "Escape" => editing.set(false),
            _ => {}
        }
    };
    let save_clicked = Callback::new(move |_| save());
    let cancel = Callback::new(move |_| editing.set(false));
    let start_editing = {
        let text = item.text.clone();
        Callback::new(move |_| {
            draft.set(text.clone());
            editing.set(true);
        })
    };

    let label = format!("Done: {}", item.text);
    let (text, author) = (item.text.clone(), item.author.clone());
    view! {
        <li class="item" class:item--done=item.done data-item=item.id.clone()>
            <input
                type="checkbox"
                class="item__check"
                aria-label=label
                prop:checked=item.done
                prop:disabled=move || !writes.get() || busy.get()
                on:change=toggle
            />
            {move || {
                if editing.get() {
                    view! {
                        <input
                            class="cl-input item__edit"
                            aria-label="Item text"
                            prop:value=move || draft.get()
                            on:input=move |event| draft.set(event_target_value(&event))
                            on:keydown=save_on_key.clone()
                        />
                        <div class="item__actions">
                            <Button size="xs" on_click=save_clicked>"Save"</Button>
                            <Button variant="subtle" size="xs" on_click=cancel>"Cancel"</Button>
                        </div>
                    }
                    .into_any()
                } else {
                    let changeable = may_change && writes.get();
                    view! {
                        <span class="item__text">{text.clone()}</span>
                        <span class="item__author">{author.clone()}</span>
                        {changeable.then(|| view! {
                            <div class="item__actions">
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

/// The field that adds an item to the end of the list.
#[component]
fn AddItem(list: Memo<String>, writer: Writer) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let submit = move |event: leptos::ev::SubmitEvent| {
        // `form-action 'none'` would stop the browser going anywhere; this
        // stops it trying.
        event.prevent_default();
        let wanted = text.get_untracked().trim().to_string();
        if wanted.is_empty() {
            return;
        }
        let list = list.get_untracked();
        writer.send(list.clone(), api::add(&list, &wanted), move |happened| {
            if happened {
                text.set(String::new());
            }
        });
    };
    view! {
        <form class="add" on:submit=submit>
            <TextInput placeholder="Add an item" value=text />
            // A button in a form submits it, so Enter in the field and a click
            // here are the same thing.
            <Button>"Add"</Button>
        </form>
    }
}
