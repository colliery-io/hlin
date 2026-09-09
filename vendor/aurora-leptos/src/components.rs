//! Aurora Dark components — core Leptos primitives.
//!
//! Static styling comes from the shared CSS classes; only the component
//! logic/markup differs. Leptos uses fine-grained signals + the `view!` macro,
//! so reactivity is expressed with closures and `Callback`s rather than
//! re-rendering whole components.

use leptos::prelude::*;

use crate::tokens::{classify, pill_bg, ApiError};

// ----------------------------------------------------------------------------
// Layout: Group / Stack
// ----------------------------------------------------------------------------

#[component]
pub fn Group(
    #[prop(optional, into)] justify: String,
    #[prop(optional)] top: bool,
    #[prop(optional)] wrap: bool,
    #[prop(optional, into)] gap: String,
    children: Children,
) -> impl IntoView {
    let mut class = String::from("cl-group");
    match justify.as_str() {
        "between" => class.push_str(" cl-group--between"),
        "end" => class.push_str(" cl-group--end"),
        _ => {}
    }
    if top {
        class.push_str(" cl-group--top");
    }
    if wrap {
        class.push_str(" cl-group--wrap");
    }
    match gap.as_str() {
        "xs" => class.push_str(" cl-group--gap-xs"),
        "sm" => class.push_str(" cl-group--gap-sm"),
        _ => {}
    }
    view! { <div class=class>{children()}</div> }
}

#[component]
pub fn Stack(
    #[prop(optional)] center: bool,
    #[prop(optional, into)] gap: String,
    children: Children,
) -> impl IntoView {
    let mut class = String::from("cl-stack");
    if center {
        class.push_str(" cl-stack--center");
    }
    match gap.as_str() {
        "xs" => class.push_str(" cl-stack--gap-xs"),
        "sm" => class.push_str(" cl-stack--gap-sm"),
        _ => {}
    }
    view! { <div class=class>{children()}</div> }
}

// ----------------------------------------------------------------------------
// Text / MONO
// ----------------------------------------------------------------------------

#[component]
pub fn Text(
    #[prop(optional, into)] size: String,
    #[prop(optional)] dimmed: bool,
    #[prop(optional)] bright: bool,
    #[prop(optional)] bold: bool,
    #[prop(optional)] mono: bool,
    children: Children,
) -> impl IntoView {
    let mut class = String::from("cl-text");
    match size.as_str() {
        "xs" => class.push_str(" cl-text--xs"),
        "sm" => class.push_str(" cl-text--sm"),
        "lg" => class.push_str(" cl-text--lg"),
        _ => {}
    }
    if dimmed {
        class.push_str(" cl-text--dimmed");
    }
    if bright {
        class.push_str(" cl-text--bright");
    }
    if bold {
        class.push_str(" cl-text--bold");
    }
    if mono {
        class.push_str(" cl-mono");
    }
    view! { <p class=class>{children()}</p> }
}

// ----------------------------------------------------------------------------
// Button
// ----------------------------------------------------------------------------

#[component]
pub fn Button(
    #[prop(default = "filled".to_string(), into)] variant: String,
    #[prop(default = "sm".to_string(), into)] size: String,
    #[prop(optional)] bad: bool,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let mut class = format!("cl-btn cl-btn--{variant}");
    if size != "sm" {
        class.push_str(&format!(" cl-btn--{size}"));
    }
    if bad {
        class.push_str(" cl-btn--bad");
    }
    view! {
        <button
            class=class
            disabled=disabled
            on:click=move |_| { if let Some(cb) = on_click { cb.run(()); } }
        >
            {children()}
        </button>
    }
}

// ----------------------------------------------------------------------------
// TextInput / Select
// ----------------------------------------------------------------------------

#[component]
pub fn TextInput(
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] placeholder: String,
    value: RwSignal<String>,
    #[prop(optional, into)] error: String,
) -> impl IntoView {
    let mut input_class = String::from("cl-input");
    if !error.is_empty() {
        input_class.push_str(" cl-input--error");
    }
    let has_label = !label.is_empty();
    let has_error = !error.is_empty();
    view! {
        <div class="cl-field">
            {has_label.then(|| view! { <label class="cl-field__label">{label}</label> })}
            <input
                class=input_class
                type="text"
                placeholder=placeholder
                prop:value=move || value.get()
                on:input=move |e| value.set(event_target_value(&e))
            />
            {has_error.then(|| view! { <span class="cl-field__error">{error}</span> })}
        </div>
    }
}

#[component]
pub fn Select(
    #[prop(optional, into)] label: String,
    options: Vec<String>,
    value: RwSignal<String>,
) -> impl IntoView {
    let has_label = !label.is_empty();
    let opts = options
        .into_iter()
        .map(|opt| view! { <option value=opt.clone()>{opt.clone()}</option> })
        .collect_view();
    view! {
        <div class="cl-field">
            {has_label.then(|| view! { <label class="cl-field__label">{label}</label> })}
            <select
                class="cl-input cl-select"
                prop:value=move || value.get()
                on:change=move |e| value.set(event_target_value(&e))
            >
                {opts}
            </select>
        </div>
    }
}

// ----------------------------------------------------------------------------
// Tooltip (pure-CSS hover)
// ----------------------------------------------------------------------------

#[component]
pub fn Tooltip(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <span class="cl-tooltip">
            {children()}
            <span class="cl-tooltip__label">{label}</span>
        </span>
    }
}

// ----------------------------------------------------------------------------
// Modal — reactive on a bool signal (fine-grained; no re-render of the tree)
// ----------------------------------------------------------------------------

#[component]
pub fn Modal(
    open: RwSignal<bool>,
    #[prop(into)] title: String,
    children: ChildrenFn,
) -> impl IntoView {
    let children = StoredValue::new(children);
    view! {
        {move || open.get().then(|| {
            let title = title.clone();
            view! {
                <div class="cl-modal-overlay" on:click=move |_| open.set(false)>
                    <div class="cl-modal" on:click=|e| e.stop_propagation()>
                        <div class="cl-modal__header">
                            <span class="cl-modal__title">{title}</span>
                            <button class="cl-modal__close" on:click=move |_| open.set(false)>"×"</button>
                        </div>
                        <div class="cl-modal__body">{children.with_value(|c| c())}</div>
                    </div>
                </div>
            }
        })}
    }
}

// ----------------------------------------------------------------------------
// Aurora Dark: Pill / StatusBadge / Dot / Panel / PageHeader / Chip
// ----------------------------------------------------------------------------

#[component]
pub fn Pill(#[prop(into)] color: String, children: Children) -> impl IntoView {
    let style = format!("background:{};color:{};", pill_bg(&color), color);
    view! { <span class="cl-pill" style=style>{children()}</span> }
}

#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let color = crate::tokens::status_color(&status);
    let style = format!("background:{};color:{};", pill_bg(color), color);
    view! { <span class="cl-status-badge" style=style>{status}</span> }
}

#[component]
pub fn Dot(
    #[prop(into)] color: String,
    #[prop(default = 8)] size: i32,
    #[prop(optional)] glow: bool,
) -> impl IntoView {
    let mut style = format!("width:{size}px;height:{size}px;background:{color};");
    if glow {
        style.push_str(&format!("box-shadow:0 0 0 3px {color}22;"));
    }
    view! { <span class="cl-dot" style=style></span> }
}

#[component]
pub fn Panel(
    #[prop(into)] title: String,
    #[prop(optional, into)] caption: String,
    children: Children,
) -> impl IntoView {
    let has_caption = !caption.is_empty();
    view! {
        <div class="cl-panel">
            <div class="cl-panel__header">
                <span class="cl-panel__title">{title}</span>
                {has_caption.then(|| view! { <span class="cl-panel__caption">{caption}</span> })}
            </div>
            {children()}
        </div>
    }
}

#[component]
pub fn PageHeader(
    #[prop(into)] title: String,
    #[prop(optional, into)] sub: String,
    #[prop(optional)] right: Option<Children>,
) -> impl IntoView {
    let has_sub = !sub.is_empty();
    view! {
        <div class="cl-page-header">
            <div>
                <div class="cl-page-header__title">{title}</div>
                {has_sub.then(|| view! { <div class="cl-page-header__sub">{sub}</div> })}
            </div>
            {right.map(|r| r())}
        </div>
    }
}

#[component]
pub fn Chip(
    #[prop(into)] label: String,
    #[prop(default = -1)] count: i32,
    #[prop(into)] active: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <button
            class="cl-chip"
            class:cl-chip--active=move || active.get()
            on:click=move |_| { if let Some(cb) = on_click { cb.run(()); } }
        >
            {label}
            {(count >= 0).then(|| view! { <span class="cl-chip__count">{count}</span> })}
        </button>
    }
}

// ----------------------------------------------------------------------------
// States: Loading / Empty / ErrorState
// ----------------------------------------------------------------------------

#[component]
pub fn Loading(#[prop(default = "Loading…".to_string(), into)] label: String) -> impl IntoView {
    view! {
        <div class="cl-center">
            <Stack center=true gap="xs">
                <div class="cl-loader"></div>
                <Text dimmed=true size="sm">{label}</Text>
            </Stack>
        </div>
    }
}

#[component]
pub fn Empty(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="cl-center">
            <Text dimmed=true>{message}</Text>
        </div>
    }
}

#[component]
pub fn ErrorState(error: ApiError, #[prop(optional)] on_retry: Option<Callback<()>>) -> impl IntoView {
    let c = classify(&error);
    let style = format!("--alert-color:var({});", c.color_var);
    let code = c.code.clone();
    view! {
        <div class="cl-alert" style=style role="alert">
            <div class="cl-alert__title">{c.title}</div>
            <Stack gap="xs">
                <Text size="sm">{c.message}</Text>
                {code.map(|code| view! { <Text size="xs" dimmed=true>{format!("code: {code}")}</Text> })}
                {c.retryable.then(|| view! {
                    // tinted to the alert color (inherits --alert-color) — no blue/red clash
                    <button
                        class="cl-btn cl-btn--xs cl-btn--alert"
                        style="width:fit-content;"
                        on:click=move |_| { if let Some(cb) = on_retry { cb.run(()); } }
                    >
                        "Retry"
                    </button>
                })}
            </Stack>
        </div>
    }
}

// ----------------------------------------------------------------------------
// Additional Mantine primitives (completing the inventory)
// ----------------------------------------------------------------------------

/// Plain block / style carrier (Mantine `Box`).
#[component]
pub fn Box(children: Children) -> impl IntoView {
    view! { <div class="cl-box">{children()}</div> }
}

/// Inline monospace `<code>` chip (Mantine `Code`).
#[component]
pub fn Code(children: Children) -> impl IntoView {
    view! { <code class="cl-code">{children()}</code> }
}

/// Accent text link (Mantine `Anchor`).
#[component]
pub fn Anchor(#[prop(optional, into)] href: String, children: Children) -> impl IntoView {
    let href = if href.is_empty() { "#".to_string() } else { href };
    view! { <a class="cl-anchor" href=href>{children()}</a> }
}

/// Hairline rule (Mantine `Divider`).
#[component]
pub fn Divider() -> impl IntoView {
    view! { <hr class="cl-divider" /> }
}

/// Standalone spinner (Mantine `Loader`).
#[component]
pub fn Loader() -> impl IntoView {
    view! { <div class="cl-loader"></div> }
}

/// Square icon-only button (Mantine `ActionIcon`).
#[component]
pub fn ActionIcon(
    #[prop(optional, into)] title: String,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            class="cl-action-icon"
            title=title
            on:click=move |_| { if let Some(cb) = on_click { cb.run(()); } }
        >
            {children()}
        </button>
    }
}

/// Tinted callout (Mantine `Alert`). `color` is a hex/token value used for the
/// accent (defaults to the bad/red token). Powers `ErrorState` too.
#[component]
pub fn Alert(
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] color: String,
    children: Children,
) -> impl IntoView {
    let style = if color.is_empty() {
        String::new()
    } else {
        format!("--alert-color:{color};")
    };
    let has_title = !title.is_empty();
    view! {
        <div class="cl-alert" style=style role="alert">
            {has_title.then(|| view! { <div class="cl-alert__title">{title}</div> })}
            {children()}
        </div>
    }
}

/// Toggle (Mantine `Switch`), controlled by a bool signal.
#[component]
pub fn Switch(
    checked: RwSignal<bool>,
    #[prop(optional, into)] label: String,
) -> impl IntoView {
    let has_label = !label.is_empty();
    view! {
        <span
            class="cl-switch"
            class:cl-switch--on=move || checked.get()
            on:click=move |_| checked.update(|v| *v = !*v)
        >
            <span class="cl-switch__track"><span class="cl-switch__thumb"></span></span>
            {has_label.then(|| view! { <span class="cl-text cl-text--sm">{label}</span> })}
        </span>
    }
}

/// Segmented selector (Mantine `SegmentedControl`), bound to a string signal.
#[component]
pub fn SegmentedControl(options: Vec<String>, value: RwSignal<String>) -> impl IntoView {
    let items = options
        .into_iter()
        .map(|opt| {
            let o = opt.clone();
            let o2 = opt.clone();
            view! {
                <button
                    class="cl-segmented__item"
                    class:cl-segmented__item--active=move || value.get() == o
                    on:click=move |_| value.set(o2.clone())
                >
                    {opt}
                </button>
            }
        })
        .collect_view();
    view! { <div class="cl-segmented">{items}</div> }
}

/// Multi-line text field (Mantine `Textarea`).
#[component]
pub fn Textarea(
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] placeholder: String,
    value: RwSignal<String>,
    #[prop(default = 4)] rows: i32,
) -> impl IntoView {
    let has_label = !label.is_empty();
    view! {
        <div class="cl-field">
            {has_label.then(|| view! { <label class="cl-field__label">{label}</label> })}
            <textarea
                class="cl-input"
                rows=rows
                placeholder=placeholder
                prop:value=move || value.get()
                on:input=move |e| value.set(event_target_value(&e))
            ></textarea>
        </div>
    }
}

/// Numeric field with steppers (Mantine `NumberInput`), bound to an f64 signal.
#[component]
pub fn NumberInput(
    #[prop(optional, into)] label: String,
    value: RwSignal<f64>,
    #[prop(default = 1.0)] step: f64,
) -> impl IntoView {
    let has_label = !label.is_empty();
    let fmt = move || {
        let v = value.get();
        if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
    };
    view! {
        <div class="cl-field">
            {has_label.then(|| view! { <label class="cl-field__label">{label}</label> })}
            <div class="cl-number">
                <input
                    class="cl-input"
                    type="number"
                    prop:value=fmt
                    on:input=move |e| {
                        if let Ok(n) = event_target_value(&e).parse::<f64>() { value.set(n); }
                    }
                />
                <div class="cl-number__steps">
                    <button class="cl-number__step" on:click=move |_| value.update(|v| *v += step)>"▲"</button>
                    <button class="cl-number__step" on:click=move |_| value.update(|v| *v -= step)>"▼"</button>
                </div>
            </div>
        </div>
    }
}

/// Password field with a reveal toggle (Mantine `PasswordInput`).
#[component]
pub fn PasswordInput(
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] placeholder: String,
    value: RwSignal<String>,
) -> impl IntoView {
    let reveal = RwSignal::new(false);
    let has_label = !label.is_empty();
    view! {
        <div class="cl-field">
            {has_label.then(|| view! { <label class="cl-field__label">{label}</label> })}
            <div class="cl-input-wrap">
                <input
                    class="cl-input"
                    type=move || if reveal.get() { "text" } else { "password" }
                    placeholder=placeholder
                    prop:value=move || value.get()
                    on:input=move |e| value.set(event_target_value(&e))
                />
                <button
                    class="cl-input-wrap__adorn"
                    type="button"
                    title="Toggle visibility"
                    on:click=move |_| reveal.update(|v| *v = !*v)
                >
                    {move || if reveal.get() {
                        view! {
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M10.585 10.587a2 2 0 0 0 2.829 2.828" />
                                <path d="M16.681 16.673a8.717 8.717 0 0 1 -4.681 1.327c-4 0 -7.333 -2.333 -10 -7c1.21 -2.12 2.554 -3.685 4.032 -4.692m3.96 -1.051a8.86 8.86 0 0 1 2.008 -.157c4 0 7.333 2.333 10 7c-.474 .83 -.969 1.542 -1.486 2.139" />
                                <path d="M3 3l18 18" />
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <circle cx="12" cy="12" r="2" />
                                <path d="M22 12c-2.667 4.667 -6 7 -10 7s-7.333 -2.333 -10 -7c2.667 -4.667 6 -7 10 -7s7.333 2.333 10 7" />
                            </svg>
                        }.into_any()
                    }}
                </button>
            </div>
        </div>
    }
}

/// Equal-width responsive grid (Mantine `SimpleGrid`).
#[component]
pub fn SimpleGrid(#[prop(default = 2)] cols: usize, children: Children) -> impl IntoView {
    let style = format!("grid-template-columns:repeat({cols},minmax(0,1fr));");
    view! { <div class="cl-simple-grid" style=style>{children()}</div> }
}

/// 12-column grid container (Mantine `Grid`). Pair with `GridCol`.
#[component]
pub fn Grid(children: Children) -> impl IntoView {
    view! { <div class="cl-grid">{children()}</div> }
}

/// A column within a `Grid`; `span` is out of 12.
#[component]
pub fn GridCol(#[prop(default = 12)] span: u8, children: Children) -> impl IntoView {
    let pct = (span.min(12) as f64 / 12.0) * 100.0;
    let style = format!("flex:0 0 {pct}%;max-width:{pct}%;");
    view! { <div class="cl-grid__col" style=style>{children()}</div> }
}

/// Bulleted list (Mantine `List`). Use `ListItem` children.
#[component]
pub fn List(children: Children) -> impl IntoView {
    view! { <ul class="cl-list">{children()}</ul> }
}

/// A `List` item.
#[component]
pub fn ListItem(children: Children) -> impl IntoView {
    view! { <li>{children()}</li> }
}

#[derive(Clone, Copy)]
struct MenuOpen(RwSignal<bool>);

/// Dropdown menu (Mantine `Menu`). `label` is the trigger text; children are
/// `MenuItem`s. Toggles on click; items close it via their handler.
#[component]
pub fn Menu(#[prop(into)] label: String, children: ChildrenFn) -> impl IntoView {
    let open = RwSignal::new(false);
    provide_context(MenuOpen(open));
    let children = StoredValue::new(children);
    view! {
        <div class="cl-menu">
            <button class="cl-btn cl-btn--default" on:click=move |_| open.update(|v| *v = !*v)>
                {label} " ▾"
            </button>
            {move || open.get().then(|| view! {
                <div class="cl-menu__dropdown">{children.with_value(|c| c())}</div>
            })}
        </div>
    }
}

/// An item inside a `Menu`. Runs `on_click` and closes the menu.
#[component]
pub fn MenuItem(
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let open = use_context::<MenuOpen>().map(|m| m.0);
    view! {
        <button
            class="cl-menu__item"
            on:click=move |_| {
                if let Some(cb) = on_click { cb.run(()); }
                if let Some(o) = open { o.set(false); }
            }
        >
            {children()}
        </button>
    }
}

/// Application scaffold (Mantine `AppShell`): a header row across the top, a left
/// navbar, and the main content area.
#[component]
pub fn AppShell(
    #[prop(optional)] header: Option<Children>,
    navbar: Children,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="cl-appshell">
            {header.map(|h| view! { <div class="cl-appshell__header">{h()}</div> })}
            <div class="cl-appshell__navbar">{navbar()}</div>
            <div class="cl-appshell__main">{children()}</div>
        </div>
    }
}

/// Button that copies `value` to the clipboard and shows a transient
/// confirmation (Mantine `CopyButton`).
#[component]
pub fn CopyButton(#[prop(into)] value: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    view! {
        <button
            class="cl-btn cl-btn--default cl-btn--xs"
            on:click=move |_| {
                if let Some(win) = web_sys::window() {
                    let _ = win.navigator().clipboard().write_text(&value);
                }
                copied.set(true);
                set_timeout(move || copied.set(false), std::time::Duration::from_millis(1200));
            }
        >
            {move || if copied.get() { "Copied!" } else { "Copy" }}
        </button>
    }
}

/// Styled table wrapper (Mantine `Table`). Compose `<thead>/<tbody>/<tr>/<th>/<td>`
/// children inside. Set `mono` for tabular monospace cells.
#[component]
pub fn Table(#[prop(optional)] mono: bool, children: Children) -> impl IntoView {
    let class = if mono { "cl-table cl-table--mono" } else { "cl-table" };
    view! { <table class=class>{children()}</table> }
}
