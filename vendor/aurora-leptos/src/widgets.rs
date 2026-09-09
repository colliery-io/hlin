//! Aurora Dark **widgets** — generic, higher-level data-display building blocks
//! that control-plane UIs compose from the core primitives.
//!
//! The vocabulary is deliberately generic dataflow: a **node** processes when its
//! **inputs** are ready. Apps map their own domain onto this (e.g. cloacina's
//! reactors → nodes, accumulators → inputs) and supply their own state labels,
//! colors, and tooltips as data — this module ships no app-specific vocab,
//! palette, or branding.
//!
//! All widgets are prop-driven. Timestamps are milliseconds-since-epoch (`f64`);
//! freshness/"time ago" is computed against `js_sys::Date::now()`.

use leptos::prelude::*;

use crate::components::{Dot, Empty, Panel, Pill, Tooltip};
use crate::tokens::token;

/// Freshness window: an input is "stale" once this long since its last event.
pub const STALE_MS: f64 = 30_000.0;

fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Compact "time ago" for a timestamp (ms since epoch). `None` → "never".
pub fn format_ago(ts: Option<f64>) -> String {
    let Some(ts) = ts else { return "never".into() };
    let s = ((now_ms() - ts) / 1000.0).round().max(0.0) as i64;
    if s < 2 {
        "just now".into()
    } else if s < 60 {
        format!("{s}s ago")
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else {
        format!("{}h ago", s / 3600)
    }
}

/// Whether a timestamp is older than the freshness window (or missing).
pub fn is_stale(last_event_at: Option<f64>) -> bool {
    match last_event_at {
        None => true,
        Some(ts) => (now_ms() - ts) > STALE_MS,
    }
}

/// Freshness as a 0–100% meter value from a timestamp (100 = just now).
pub fn freshness_pct(last_event_at: Option<f64>) -> f64 {
    match last_event_at {
        Some(ts) => (100.0 - ((now_ms() - ts) / STALE_MS) * 100.0).clamp(4.0, 100.0),
        None => 0.0,
    }
}

// ----------------------------------------------------------------------------
// Generic primitives
// ----------------------------------------------------------------------------

/// A thin horizontal progress / freshness bar. `value` is 0–100; `color`
/// defaults to the ok/green token.
#[component]
pub fn Meter(value: f64, #[prop(optional, into)] color: String) -> impl IntoView {
    let color = if color.is_empty() { token::OK.to_string() } else { color };
    let pct = value.clamp(0.0, 100.0);
    view! {
        <div class="cl-meter">
            <div class="cl-meter__fill" style=format!("width:{pct}%;background:{color};")></div>
        </div>
    }
}

/// A tinted callout banner. `color` sets the accent (defaults to gold/warn);
/// `icon` defaults to a warning triangle.
#[component]
pub fn Banner(
    #[prop(optional, into)] color: String,
    #[prop(optional, into)] icon: String,
    children: Children,
) -> impl IntoView {
    let color = if color.is_empty() { token::GOLD.to_string() } else { color };
    let icon = if icon.is_empty() { "⚠".to_string() } else { icon };
    view! {
        <div class="cl-banner" style=format!("--banner-color:{color};")>
            <span class="cl-banner__icon">{icon}</span>
            <span class="cl-banner__text">{children()}</span>
        </div>
    }
}

/// One state bucket for [`StateCounts`].
#[derive(Clone, PartialEq)]
pub struct StateCount {
    pub label: String,
    pub count: usize,
    pub color: String,
}

/// A row of colored count badges, one per state (e.g. run states). Each shows a
/// tooltip of `"{count} {label}"`.
#[component]
pub fn StateCounts(counts: Vec<StateCount>) -> impl IntoView {
    let items = counts
        .into_iter()
        .map(|s| {
            view! {
                <Tooltip label=format!("{} {}", s.count, s.label)>
                    <span class="cl-state-count" style=format!("background:{};", s.color)>{s.count}</span>
                </Tooltip>
            }
        })
        .collect_view();
    view! { <div class="cl-group cl-group--gap-xs cl-group--wrap">{items}</div> }
}

/// A status/health pill with an optional explanatory tooltip. The caller supplies
/// the display `label`, `color`, and `tip` (no built-in vocab).
#[component]
pub fn HealthPill(
    #[prop(into)] label: String,
    #[prop(into)] color: String,
    #[prop(optional, into)] tip: String,
) -> impl IntoView {
    if tip.is_empty() {
        view! { <Pill color=color>{label}</Pill> }.into_any()
    } else {
        view! { <Tooltip label=tip><Pill color=color>{label}</Pill></Tooltip> }.into_any()
    }
}

// ----------------------------------------------------------------------------
// BuildStatusBadge — generic CI/CD build state (success/building/failed/pending)
// ----------------------------------------------------------------------------

fn build_status_color(status: &str) -> &'static str {
    match status {
        "success" => token::OK,
        "failed" => token::BAD,
        "building" => token::ICE,
        "pending" => token::MUTED,
        _ => token::MUTED,
    }
}

#[component]
pub fn BuildStatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let color = build_status_color(&status).to_string();
    view! { <Pill color=color>{status}</Pill> }
}

// ----------------------------------------------------------------------------
// Input model + data-display widgets (node / inputs)
// ----------------------------------------------------------------------------

/// A data input feeding a node. Display fields (`state_label`, `state_color`,
/// `rate`) are app-provided — the widgets render, the app supplies semantics.
#[derive(Clone, Default, PartialEq)]
pub struct Input {
    pub name: String,
    /// The node/group this input belongs to (shown as a caption where relevant).
    pub group: Option<String>,
    /// App display label for the input's state (e.g. "live", "stale").
    pub state_label: String,
    /// App display color for the state (hex or CSS var).
    pub state_color: String,
    /// Last event, ms since epoch. Drives freshness + staleness.
    pub last_event_at: Option<f64>,
    /// Pre-formatted throughput text (e.g. "~18240/min").
    pub rate: Option<String>,
    pub error: Option<String>,
    /// Count of manual events pushed to this input (shows a "manual" badge).
    pub manual_events: Option<i64>,
    pub last_manual_event_at: Option<f64>,
}

/// Banner that appears when any input is stale (time-based). Renders nothing when
/// all inputs are fresh. Built on [`Banner`].
#[component]
pub fn StaleInputsBanner(inputs: Vec<Input>) -> impl IntoView {
    let stale: Vec<&Input> = inputs.iter().filter(|i| is_stale(i.last_event_at)).collect();
    if stale.is_empty() {
        return view! {}.into_any();
    }
    let n = stale.len();
    let names = stale.iter().map(|i| i.name.clone()).collect::<Vec<_>>().join(", ");
    let oldest_name = stale[0].name.clone();
    let ago = format_ago(stale[0].last_event_at);
    let s = if n == 1 { "" } else { "s" };
    let have = if n == 1 { "has" } else { "have" };
    let lead = format!("{n} input{s} stale — ");
    let tail = format!(" {have} no recent data ({oldest_name} last seen {ago}).");
    view! {
        <Banner>
            {lead}
            <b>{names}</b>
            {tail}
        </Banner>
    }
    .into_any()
}

/// Readiness panel for a node: its trigger description, a per-input fresh/stale
/// list, and a ready/waiting summary. `mode_label`/`strategy_label` are
/// app-supplied words (e.g. "all"/"latest"); `require_all` drives the
/// waiting-vs-ready summary copy.
#[component]
pub fn NodeReadiness(
    #[prop(optional, into)] node: String,
    #[prop(optional, into)] mode_label: String,
    #[prop(optional, into)] strategy_label: String,
    #[prop(optional)] require_all: bool,
    inputs: Vec<Input>,
    #[prop(optional)] last_run_at: Option<f64>,
) -> impl IntoView {
    let total = inputs.len();
    let ready = inputs.iter().filter(|i| !is_stale(i.last_event_at)).count();

    let rows = inputs
        .iter()
        .map(|i| {
            let fresh = !is_stale(i.last_event_at);
            let mark = if fresh { "✓" } else { "⚠" };
            let mark_color = if fresh { token::OK } else { token::GOLD };
            let hint = if fresh { "fresh · ready" } else { "no data · stale" };
            let hint_color = if fresh { "var(--faint)" } else { token::GOLD };
            view! {
                <div class="cl-readiness__row">
                    <span style=format!("color:{mark_color};")>{mark}</span>
                    <span class="cl-readiness__name">{i.name.clone()}</span>
                    <span class="cl-readiness__hint" style=format!("color:{hint_color};")>{hint}</span>
                </div>
            }
        })
        .collect_view();

    let summary_color = if ready == total { token::OK } else { token::GOLD };
    let summary = if require_all && ready < total {
        format!("Waiting on {} of {} inputs", total - ready, total)
    } else {
        format!("Ready on {ready} of {total} inputs")
    };
    let last = format_ago(last_run_at);

    view! {
        <Panel title="Readiness" caption=node>
            <div class="cl-readiness">
                <div class="cl-readiness__desc">
                    "Runs when "
                    <b style=format!("color:{};", token::VIOLET)>{mode_label}</b>
                    " of its inputs have new data, using each input's "
                    <b style=format!("color:{};", token::TEAL)>{strategy_label}</b>
                    " value."
                </div>
                <div class="cl-readiness__list">{rows}</div>
                <div class="cl-readiness__summary">
                    <div class="cl-readiness__summary-title" style=format!("color:{summary_color};")>{summary}</div>
                    <div class="cl-readiness__summary-sub">{format!("last run {last}")}</div>
                </div>
            </div>
        </Panel>
    }
}

/// A freshness table over a node's inputs: state, last event, rate, a freshness
/// [`Meter`], an optional per-row action, and an error line. App supplies state
/// label/color; freshness/staleness are computed from `last_event_at`.
#[component]
pub fn InputTable(
    inputs: Vec<Input>,
    #[prop(optional)] on_action: Option<Callback<String>>,
    #[prop(default = "action ▸".to_string(), into)] action_label: String,
) -> impl IntoView {
    if inputs.is_empty() {
        return view! { <Empty message="No inputs connected." /> }.into_any();
    }

    let rows = inputs
        .into_iter()
        .map(|i| {
            let stale = is_stale(i.last_event_at);
            let pct = freshness_pct(i.last_event_at);
            let color = i.state_color.clone();
            let last_event = format_ago(i.last_event_at);
            let last_event_color = if stale { token::GOLD } else { "var(--muted)" };
            let bar_color = if stale { token::BAD.to_string() } else { token::OK.to_string() };
            let rate = i.rate.clone().unwrap_or_else(|| "—".into());
            let injects = i.manual_events.unwrap_or(0);
            let name = i.name.clone();
            let error = i.error.clone();

            let manual = (injects > 0).then(|| {
                let p = if injects == 1 { "" } else { "s" };
                let last = i.last_manual_event_at.map(|t| format!(" · last {}", format_ago(Some(t)))).unwrap_or_default();
                view! {
                    <Tooltip label=format!("{injects} manual event{p}{last}")>
                        <span style="flex:none;"><Pill color=token::GOLD.to_string()>"manual"</Pill></span>
                    </Tooltip>
                }
            });

            let action = match on_action {
                Some(cb) => {
                    let n = name.clone();
                    let lbl = action_label.clone();
                    view! { <button class="cl-input-table__action" on:click=move |_| cb.run(n.clone())>{lbl}</button> }.into_any()
                }
                None => view! { <span></span> }.into_any(),
            };

            view! {
                <div class="cl-input-table__row">
                    <div class="cl-input-table__grid">
                        <span class="cl-input-table__name-cell">
                            <Dot color=color.clone() size=7 />
                            <span class="cl-input-table__name">{name}</span>
                            {manual}
                        </span>
                        <span class="cl-input-table__state" style=format!("color:{color};")>{i.state_label.clone()}</span>
                        <span class="cl-input-table__cell-mono" style=format!("color:{last_event_color};")>{last_event}</span>
                        <span class="cl-input-table__cell-mono" style="color:var(--muted);text-align:right;">{rate}</span>
                        <Meter value=pct color=bar_color />
                        {action}
                    </div>
                    {error.map(|e| view! {
                        <div class="cl-input-table__error">
                            <span style=format!("color:{};", token::BAD)>"✕ error"</span>
                            " "
                            <span style="color:#b97a7a;">{e}</span>
                        </div>
                    })}
                </div>
            }
        })
        .collect_view();

    view! {
        <div>
            <div class="cl-input-table__head">
                <span class="cl-input-table__th">"Input"</span>
                <span class="cl-input-table__th">"State"</span>
                <span class="cl-input-table__th">"Last event"</span>
                <span class="cl-input-table__th cl-input-table__th--right">"Rate"</span>
                <span class="cl-input-table__th">"Freshness"</span>
                <span class="cl-input-table__th"></span>
            </div>
            {rows}
        </div>
    }
    .into_any()
}
