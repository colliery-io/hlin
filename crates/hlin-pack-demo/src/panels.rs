//! The panels themselves.
//!
//! Each takes data and a context and draws. None of them fetch, hold state, or
//! know anything about the stream: a panel is a function of what it was given,
//! which is what makes them testable by rendering to a string.

use hlin_manifest::Envelope;
use hlin_manifest::envelope::{Health, Options, Records, Scalar, Series, Status};
use hlin_view::Kind;
use hlin_view::pack::Context;
use hlin_view::render::Treatment;
use leptos::prelude::*;

use crate::ROOT_CLASS;
use crate::chart::{self, Bounds, HEIGHT, WIDTH};
use crate::format;

/// The wrapper every panel shares: the root class, the treatment's styling,
/// and the age or reason where there is one.
fn frame(context: Context, body: AnyView) -> AnyView {
    let treatment = match context.treatment {
        Treatment::Aged => "aged",
        Treatment::Dimmed => "dimmed",
        _ => "",
    };
    let class = format!("{ROOT_CLASS} {treatment}");

    let note = match (context.treatment, context.notice, context.age_seconds) {
        // A dimmed panel is showing data that may be wrong, so the reason
        // matters more than the age.
        (Treatment::Dimmed, Some(notice), _) => Some(format!("unavailable: {}", notice.cause)),
        (Treatment::Aged, _, Some(seconds)) => Some(format::age(seconds)),
        (Treatment::Aged, _, None) => Some("not current".to_string()),
        _ => None,
    };

    view! {
        <div class=class>
            {body}
            {note.map(|text| view! { <div class="note">{text}</div> })}
        </div>
    }
    .into_any()
}

/// One value.
pub fn stat(data: &Scalar, context: Context) -> AnyView {
    let unit = data.unit.as_deref();
    let shown = match &data.value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .map(|value| format::value(value, unit))
            .unwrap_or_else(|| number.to_string()),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Null => "no value".to_string(),
        other => other.to_string(),
    };

    let delta = data
        .value
        .as_f64()
        .zip(data.previous)
        .and_then(|(current, previous)| format::delta(current, previous));

    frame(
        context,
        view! {
            <div class="value">{shown}</div>
            {data.label.clone().map(|text| view! { <div class="label">{text}</div> })}
            {delta.map(|text| view! { <div class="label">{text}</div> })}
        }
        .into_any(),
    )
}

/// A line chart with a time axis and a legend.
pub fn timeseries(data: &Series, context: Context) -> AnyView {
    let Some(bounds) = Bounds::of(data) else {
        return frame(
            context,
            view! { <div class="placeholder">"no data in this range"</div> }.into_any(),
        );
    };

    let unit = data.unit.clone();
    let lines: Vec<AnyView> = data
        .series
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let path = chart::path(line, bounds);
            let colour = chart::colour(index);
            view! {
                <path d=path fill="none" stroke=colour stroke-width="1.6"
                      stroke-linejoin="round" stroke-linecap="round" />
            }
            .into_any()
        })
        .collect();

    let legend: Vec<AnyView> = data
        .series
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let colour = chart::colour(index);
            let name = line.name.clone();
            view! {
                <span>
                    <i class="swatch" style=format!("background:{colour}")></i>
                    {name}
                </span>
            }
            .into_any()
        })
        .collect();

    let high = format::value(bounds.high, unit.as_deref());
    let low = format::value(bounds.low, unit.as_deref());

    frame(
        context,
        view! {
            <svg viewBox=format!("0 0 {WIDTH} {HEIGHT}") width="100%" role="img">
                <line x1="28" y1=HEIGHT - 28.0 x2=WIDTH - 28.0 y2=HEIGHT - 28.0
                      stroke="#dee2e6" stroke-width="1" />
                <text x="2" y="20" font-size="10" fill="#868e96">{high}</text>
                <text x="2" y=HEIGHT - 32.0 font-size="10" fill="#868e96">{low}</text>
                {lines}
            </svg>
            <div class="legend">{legend}</div>
        }
        .into_any(),
    )
}

/// A compact line per series, with its latest value.
pub fn sparkline(data: &Series, context: Context) -> AnyView {
    let unit = data.unit.clone();
    let rows: Vec<AnyView> = data
        .series
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let path = chart::spark_path(line, 120.0, 24.0);
            let colour = chart::colour(index);
            let latest = chart::latest(line)
                .map(|value| format::value(value, unit.as_deref()))
                .unwrap_or_else(|| "no data".to_string());
            let name = line.name.clone();

            view! {
                <div class="spark">
                    <svg viewBox="0 0 120 24" width="120" height="24" role="img">
                        <path d=path fill="none" stroke=colour stroke-width="1.4"
                              stroke-linejoin="round" stroke-linecap="round" />
                    </svg>
                    <span class="value" style="font-size:15px">{latest}</span>
                    <span class="label">{name}</span>
                </div>
            }
            .into_any()
        })
        .collect();

    frame(context, view! { <div>{rows}</div> }.into_any())
}

/// A table of records.
pub fn table(data: &Records, context: Context) -> AnyView {
    let headers: Vec<AnyView> = data
        .columns
        .iter()
        .map(|column| {
            let label = column.label.clone();
            view! { <th>{label}</th> }.into_any()
        })
        .collect();

    let rows: Vec<AnyView> = data
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<AnyView> = data
                .columns
                .iter()
                .map(|column| {
                    // A missing key renders empty; an undeclared key is
                    // ignored, because a platform adding a field must not
                    // break a panel.
                    let text = row
                        .get(&column.key)
                        .map(|value| cell(value, column.unit.as_deref()))
                        .unwrap_or_default();
                    view! { <td>{text}</td> }.into_any()
                })
                .collect();
            view! { <tr>{cells}</tr> }.into_any()
        })
        .collect();

    frame(
        context,
        view! {
            <table>
                <thead><tr>{headers}</tr></thead>
                <tbody>{rows}</tbody>
            </table>
        }
        .into_any(),
    )
}

fn cell(value: &serde_json::Value, unit: Option<&str>) -> String {
    match value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .map(|inner| format::value(inner, unit))
            .unwrap_or_else(|| number.to_string()),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A time series as a table: one row per instant, one column per series.
pub fn series_as_table(data: &Series, context: Context) -> AnyView {
    // Every instant any series reported, in order, so a series with gaps still
    // lines up with the others.
    let mut instants: Vec<i64> = data
        .series
        .iter()
        .flat_map(|line| line.points.iter().map(|point| point.at()))
        .collect();
    instants.sort_unstable();
    instants.dedup();

    let unit = data.unit.clone();
    let headers: Vec<AnyView> = data
        .series
        .iter()
        .map(|line| {
            let name = line.name.clone();
            view! { <th>{name}</th> }.into_any()
        })
        .collect();

    let rows: Vec<AnyView> = instants
        .iter()
        .map(|at| {
            let when = chrono::DateTime::from_timestamp_millis(*at)
                .map(|instant| format::instant(&instant))
                .unwrap_or_default();

            let cells: Vec<AnyView> = data
                .series
                .iter()
                .map(|line| {
                    let text = line
                        .points
                        .iter()
                        .find(|point| point.at() == *at)
                        .and_then(|point| point.value())
                        .map(|value| format::value(value, unit.as_deref()))
                        .unwrap_or_default();
                    view! { <td>{text}</td> }.into_any()
                })
                .collect();

            view! { <tr><td>{when}</td>{cells}</tr> }.into_any()
        })
        .collect();

    frame(
        context,
        view! {
            <table>
                <thead><tr><th>"Time"</th>{headers}</tr></thead>
                <tbody>{rows}</tbody>
            </table>
        }
        .into_any(),
    )
}

fn badge(health: Health) -> AnyView {
    let (class, text) = match health {
        Health::Ok => ("badge ok", "ok"),
        Health::Degraded => ("badge degraded", "degraded"),
        Health::Down => ("badge down", "down"),
        Health::Unknown => ("badge unknown", "unknown"),
    };
    view! { <span class=class>{text}</span> }.into_any()
}

/// A health rollup and its parts.
pub fn status(data: &Status, context: Context) -> AnyView {
    let items: Vec<AnyView> = data
        .items
        .iter()
        .map(|item| {
            let name = item.name.clone();
            let detail = item.detail.clone();
            view! {
                <tr>
                    <td>{name}</td>
                    <td>{badge(item.status)}</td>
                    <td class="label">{detail}</td>
                </tr>
            }
            .into_any()
        })
        .collect();

    frame(
        context,
        view! {
            <div>
                {badge(data.status)}
                " "
                <strong>{data.label.clone()}</strong>
            </div>
            {data.detail.clone().map(|text| view! { <div class="label">{text}</div> })}
            {(!data.items.is_empty()).then(|| view! {
                <table><tbody>{items}</tbody></table>
            })}
        }
        .into_any(),
    )
}

/// The choices a parameter offers, drawn as a panel.
pub fn options(data: &Options, context: Context) -> AnyView {
    if data.options.is_empty() {
        return frame(
            context,
            view! { <div class="placeholder">"no options"</div> }.into_any(),
        );
    }

    let rows: Vec<AnyView> = data
        .options
        .iter()
        .map(|choice| {
            let label = choice.label.clone();
            let value = choice.value.clone();
            view! { <tr><td>{label}</td><td class="label">{value}</td></tr> }.into_any()
        })
        .collect();

    frame(
        context,
        view! { <table><tbody>{rows}</tbody></table> }.into_any(),
    )
}

/// Any document, plainly.
///
/// The guarantee that nothing is undrawable. Deliberately unattractive.
pub fn raw(data: &Envelope, context: Context) -> AnyView {
    let name = data.name();
    let body = serde_json::to_string_pretty(data)
        .unwrap_or_else(|_| "this document could not be shown".to_string());

    frame(
        context,
        view! {
            <div class="label">{name}</div>
            <pre>{body}</pre>
        }
        .into_any(),
    )
}

/// The shape a kind will fill, while nothing has arrived.
pub fn skeleton(kind: Kind, context: Context) -> AnyView {
    // Shaped roughly like what is coming, so the panel does not jump when it
    // arrives.
    let bars: Vec<AnyView> = match kind {
        Kind::Stat => vec![(140.0, 30.0), (90.0, 12.0)],
        Kind::Timeseries => vec![(WIDTH, HEIGHT)],
        Kind::Sparkline => vec![(220.0, 24.0), (220.0, 24.0)],
        Kind::Table | Kind::Status | Kind::Raw => {
            vec![(320.0, 14.0), (300.0, 14.0), (280.0, 14.0)]
        }
    }
    .into_iter()
    .map(|(width, height)| {
        view! {
            <div class="skeleton"
                 style=format!("width:{width}px;max-width:100%;height:{height}px;margin:4px 0")>
            </div>
        }
        .into_any()
    })
    .collect();

    frame(context, view! { <div>{bars}</div> }.into_any())
}

/// Nothing can be shown, and why.
pub fn placeholder(_kind: Kind, context: Context) -> AnyView {
    let (reason, successor) = match context.notice {
        Some(notice) => (
            explain(notice.cause),
            notice
                .offer_successor
                .then_some("a replacement may be available"),
        ),
        None => ("nothing to show", None),
    };

    frame(
        context,
        view! {
            <div class="placeholder">
                <div class="reason">{reason}</div>
                {successor.map(|text| view! { <div class="label">{text}</div> })}
            </div>
        }
        .into_any(),
    )
}

/// What a cause means, in words for the person looking at the panel.
///
/// Written by the shell rather than passed through from a platform, so nothing
/// a platform wrote reaches a viewer.
fn explain(cause: hlin_view::Cause) -> &'static str {
    use hlin_view::Cause;
    match cause {
        // Deliberately does not say who. A pack is handed a cause and nothing
        // else, and the unreachable party is sometimes a platform and sometimes
        // Hlin itself — a panel that named the wrong one would send a person to
        // go and look at a system that is answering perfectly well. Whoever
        // knows writes it in the detail beneath.
        Cause::Unreachable => "no answer",
        Cause::Malformed => "this platform sent something unreadable",
        Cause::Unknown => "this panel no longer exists",
        Cause::Deprecated => "this panel has been retired",
        Cause::Forbidden => "you do not have access to this panel",
    }
}
