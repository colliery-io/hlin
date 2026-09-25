//! The sparkline's module: one metric as a line over the surface's time
//! range, with where it is now, and its low and high.
//!
//! The range is the shell's: its time picker, sent in `context`. Changing the
//! picker changes the context, which fetches again (`Widget::load` does that
//! for every widget); the request here reads the range to ask for it. On a
//! page, which has no picker, there is no range, and the widget answers the
//! last hour; the module asks again each minute so that hour moves on.

use std::time::Duration;

use hlin_module::Request;
use hlin_widget_module::{Widget, loaded_view, start};
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Drawn {
    metric: String,
    label: String,
    unit: String,
    step_ms: i64,
    points: Vec<(i64, f64)>,
    metrics: Vec<(String, String)>,
}

/// The SVG `points` for a line through `values`, in a 100 by 30 box, with the
/// low at the bottom and the high at the top.
fn polyline(values: &[f64]) -> String {
    let (low, high) = bounds(values);
    let span = (high - low).max(f64::EPSILON);
    let last = (values.len().max(2) - 1) as f64;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 / last * 100.0;
            let y = 28.0 - (value - low) / span * 26.0;
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn bounds(values: &[f64]) -> (f64, f64) {
    values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| {
            (low.min(*value), high.max(*value))
        })
}

/// A value as a person reads it: no more decimals than it deserves.
fn shown(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}")
    } else if value >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn main() {
    start("sparkline", |widget: Widget| {
        let context = widget.module().context();
        let visible = widget.module().visible();
        let tick = RwSignal::new(0_u32);
        set_interval(
            move || {
                if visible.get_untracked() && context.with_untracked(|c| c.time_range.is_none()) {
                    tick.update(|tick| *tick += 1);
                }
            },
            Duration::from_secs(60),
        );
        let drawn = widget.load::<Drawn>(move || {
            tick.track();
            let request = Request::get("/api/sparkline");
            match context.with(|context| context.time_range) {
                Some(range) => request.query(format!(
                    "from_ms={}&to_ms={}",
                    range.from_millis, range.to_millis
                )),
                None => request,
            }
        });
        let read_only = widget.read_only();

        loaded_view(drawn, move |drawn| {
            let values: Vec<f64> = drawn.points.iter().map(|(_, value)| *value).collect();
            let (low, high) = bounds(&values);
            let latest = values.last().copied();
            let unit = drawn.unit.clone();
            let choices = drawn
                .metrics
                .iter()
                .map(|(id, label)| {
                    let chosen = *id == drawn.metric;
                    let id = id.clone();
                    view! {
                        <button
                            class="w-button sparkline__metric"
                            class:w-button--quiet=!chosen
                            aria-pressed=chosen.to_string()
                            disabled=move || read_only.get() || chosen
                            on:click=move |_| {
                                widget.send(
                                    Request::put("/api/sparkline/metric")
                                        .json(&serde_json::json!({ "metric": id }))
                                        .expect("a string serialises"),
                                )
                            }
                        >
                            {label.clone()}
                        </button>
                    }
                })
                .collect_view();
            let line = (values.len() > 1).then(|| {
                view! {
                    <svg class="sparkline__chart" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
                        <polyline class="sparkline__line" points=polyline(&values) />
                    </svg>
                }
            });
            let empty = (values.len() <= 1).then(|| {
                view! { <p class="w-quiet sparkline__empty">"Nothing in this range yet."</p> }
            });
            view! {
                <div class="sparkline" data-metric=drawn.metric.clone() data-points=values.len() data-step=drawn.step_ms>
                    <div class="sparkline__now">
                        <span class="w-big">{latest.map(shown).unwrap_or_else(|| "–".to_string())}</span>
                        <span class="w-quiet">{format!("{} · {}", drawn.label, unit)}</span>
                    </div>
                    {line}
                    {empty}
                    <p class="w-quiet sparkline__bounds">
                        {latest.map(|_| format!("low {} · high {}", shown(low), shown(high)))}
                    </p>
                </div>
                <div class="w-row">{choices}</div>
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_fills_the_box_low_at_the_bottom_high_at_the_top() {
        assert_eq!(
            polyline(&[1.0, 3.0, 2.0]),
            "0.00,28.00 50.00,2.00 100.00,15.00"
        );
    }

    #[test]
    fn a_flat_line_is_still_a_line() {
        assert_eq!(polyline(&[5.0, 5.0]), "0.00,28.00 100.00,28.00");
    }

    #[test]
    fn values_have_the_decimals_they_deserve() {
        assert_eq!(shown(421.37), "421");
        assert_eq!(shown(42.137), "42.1");
        assert_eq!(shown(1.2345), "1.23");
    }
}
