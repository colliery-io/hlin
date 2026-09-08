//! Aurora Dark as a [Hlin](https://github.com/colliery-io/hlin) design pack.
//!
//! Behind the `hlin` feature, off by default. With it off this crate has no
//! idea Hlin exists; with it on, [`AuroraPack`] implements `DesignPack` and a
//! Hlin front end draws every platform's panels with Aurora's components.
//!
//! The arrangement is deliberate. Hlin owns the vocabulary — six view kinds,
//! five envelope shapes, four panel states — and never learns which design
//! system draws them. Aurora owns how its own components are driven and never
//! learns what a manifest is. This module is the only place the two meet, and
//! it is the design system's to keep, because driving Aurora's components is
//! Aurora's business.
//!
//! Two of Aurora's own rules do most of the work here. Colour comes from
//! [`crate::tokens::token`], never a literal, so a panel drawn in Hlin matches
//! one drawn in any other Colliery app. And the pack renders while the app
//! supplies meaning: Aurora takes a colour and a label and draws them, while
//! deciding that a degraded service is gold is Hlin's judgement, made below.
//!
//! What Aurora does not have is a chart, so [`AuroraPack::timeseries`] and
//! [`AuroraPack::sparkline`] draw SVG by hand. Every stroke is a token, which
//! keeps them consistent with everything around them without asking a general
//! design system to grow a plotting library.

use hlin_view::envelope::{Envelope, Health, Options, Records, Scalar, Series, Status};
use hlin_view::pack::Context;
use hlin_view::{Cause, DesignPack, Intent, Kind};
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;

use crate::components::{Alert, Code, Dot, Empty, Group, List, ListItem, Pill, Stack, Table, Text};
use crate::tokens::token;
use crate::widgets::HealthPill;
use crate::{Graph, GraphEdge, GraphNode, Meter};

/// The stylesheet a Hlin front end needs to draw with this pack.
///
/// Aurora's own CSS unchanged, then the little that belongs to Hlin: the two
/// charts drawn here rather than by a component, the aged treatment, and the
/// mapping onto the custom properties Hlin's own chrome reads so the frame
/// around the panels is dark too.
pub const HLIN_CSS: &str = concat!(
    include_str!("../style/fonts.css"),
    "\n",
    include_str!("../style/tokens.css"),
    "\n",
    include_str!("../style/components.css"),
    "\n",
    include_str!("hlin.css"),
);

thread_local! {
    /// Where a brush started, if one is in progress.
    ///
    /// Deliberately not component state. A panel redraws whenever a frame
    /// arrives — constantly, on a shell tuned for live data — and a
    /// `StoredValue` is created fresh by each render, so a redraw between
    /// pointerdown and pointerup silently threw the drag away. The same shape of
    /// bug as a pointer capture that does not survive its element being
    /// recreated.
    ///
    /// A thread local is right here rather than merely convenient: wasm is
    /// single threaded, a person has one pointer down at a time, and the value
    /// is meaningless outside the moment between press and release.
    static BRUSH_START: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

/// Aurora Dark, as a Hlin design pack.
#[derive(Debug, Clone, Copy, Default)]
pub struct AuroraPack;

impl AuroraPack {
    /// A panel body with whatever this context calls for underneath it.
    ///
    /// Age and reason are shown here rather than in each method, so no kind can
    /// forget to say that what it is showing is not current.
    fn framed(context: Context, body: AnyView) -> AnyView {
        let aged = context.is_aged();
        let age = context.age_seconds.filter(|_| aged).map(ago);
        let reason = context.notice.map(|notice| explain(notice.cause));

        view! {
            <div class="hlin" class:hlin--aged=aged>{body}</div>
            {age.map(|text| view! {
                <Group gap="xs">
                    <Dot color=token::GOLD size=6 />
                    <Text size="xs" dimmed=true>{text}</Text>
                </Group>
            })}
            {reason.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
        }
        .into_any()
    }

    /// `aurora.meter`: a scalar as a proportion of something.
    ///
    /// Aurora's `Meter` wants 0–100, and a `scalar.v1` carries no maximum, so
    /// one has to come from somewhere. `extra` is where an envelope keeps what
    /// the vocabulary has no field for, and reading `max` out of it is this
    /// component's own convention between a platform and this pack — exactly
    /// the kind of agreement that has no business in Hlin's contract.
    ///
    /// Without a `max` the meter falls back to treating the value as a
    /// percentage, which is right for the common case of asking for one.
    fn meter(data: &Scalar, context: Context) -> AnyView {
        let value = data.value.as_f64().unwrap_or(0.0);
        let max = data
            .extra
            .get("max")
            .and_then(serde_json::Value::as_f64)
            .filter(|max| *max > 0.0)
            .unwrap_or(100.0);

        let percent = (value / max * 100.0).clamp(0.0, 100.0);
        let colour = if percent >= 90.0 {
            token::BAD
        } else if percent >= 75.0 {
            token::GOLD
        } else {
            token::OK
        };

        let label = data.label.clone();
        let shown = format!("{}", (percent).round());

        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    <Group gap="xs">
                        <Text size="lg">{shown}"%"</Text>
                        {label.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
                    </Group>
                    <Meter value=percent color=colour.to_string() />
                </Stack>
            }
            .into_any(),
        )
    }

    /// `aurora.faceted`: a chart with its filter drawn beside it, in Aurora's
    /// own controls rather than the shell's.
    ///
    /// This is the round trip in one component, and it is worth being precise
    /// about which parts are which. Coming in: `context.controls` carries what
    /// the platform said it will accept and what is chosen now — the shell
    /// fetched the first on the viewer's behalf, because a browser cannot call
    /// a platform directly. Going out: clicking a pill sends
    /// [`Intent::Select`], which lands in the same place the shell's own
    /// control writes to, so the two can never disagree about what is set.
    ///
    /// Nothing here is Hlin-specific beyond reading that vocabulary. Aurora
    /// decides a filter is a row of pills; the shell has no opinion and is not
    /// told.
    fn faceted(data: &Series, context: Context) -> AnyView {
        // The first control that offers a choice. A panel with none gets the
        // chart alone, which is the honest thing to draw: pills over an empty
        // list would be a filter that filters nothing.
        let control = context
            .controls
            .iter()
            .find(|control| !control.choices.is_empty())
            .cloned();

        // The same chart `timeseries` draws, from the same two helpers, so the
        // filtered view and the unfiltered one are the same picture.
        let unit = data.unit.clone();
        let drawn = chart(plot(data));
        let chart_view = view! {
            <Stack gap="xs">
                {unit.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
                {drawn}
            </Stack>
        }
        .into_any();

        let Some(control) = control else {
            return Self::framed(context, chart_view);
        };

        let chosen = control.value().unwrap_or_default().to_string();
        let param = control.id.clone();
        let live = context.emit.is_live();

        // One pill per choice, plus one to put it back to the platform's
        // default. Rendered as static text where nothing is listening, because
        // something that looks clickable and is not is worse than plain.
        let pills = control
            .choices
            .iter()
            .map(|choice| (choice.value.clone(), choice.label.clone()))
            .chain(std::iter::once((String::new(), "Any".to_string())))
            .map(|(value, label)| {
                let selected = value == chosen;
                let emit = context.emit.clone();
                let param = param.clone();
                let sent = value.clone();

                view! {
                    <button
                        class="cl-hlin-facet"
                        class:cl-hlin-facet--on=selected
                        disabled=!live
                        on:click=move |_| {
                            emit.send(Intent::Select {
                                param: param.clone(),
                                // An empty value clears the choice rather than
                                // sending "" as one, which is what the shell's
                                // own control does with a blank box.
                                values: if sent.is_empty() {
                                    vec![]
                                } else {
                                    vec![sent.clone()]
                                },
                            });
                        }
                    >
                        {label}
                    </button>
                }
            })
            .collect_view();

        let title = control.label.clone();
        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    <Group gap="xs">
                        <Text size="xs" dimmed=true>{title}</Text>
                        {pills}
                    </Group>
                    {chart_view}
                </Stack>
            }
            .into_any(),
        )
    }

    /// `aurora.brush`: a chart a person can drag a time range out of.
    ///
    /// The other direction of the round trip. `aurora.faceted` sends
    /// [`Intent::Select`], which changes one panel; this sends
    /// [`Intent::Range`], which changes the surface — every panel beside it
    /// moves to the window that was dragged.
    ///
    /// The mapping is the element's own box rather than the SVG's coordinate
    /// system, because the chart is drawn with `preserveAspectRatio="none"` and
    /// stretches to whatever width it is given. A fraction across the box is a
    /// fraction across the window either way, and it survives a resize without
    /// anything having to be recomputed.
    fn brush(data: &Series, context: Context) -> AnyView {
        // The window the drawn points actually span. A drag is a fraction of
        // this, so a chart of the last hour yields a range inside that hour
        // rather than one invented from the axis.
        let (first, last) = data
            .series
            .iter()
            .flat_map(|line| line.points.iter())
            .fold((i64::MAX, i64::MIN), |(first, last), point| {
                (first.min(point.0), last.max(point.0))
            });

        let unit = data.unit.clone();
        let drawn = chart(plot(data));

        // Nothing to brush out of a chart with no points, and a surface that
        // moved to an empty range would be a worse answer than not moving.
        if first >= last || !context.emit.is_live() {
            return Self::framed(
                context,
                view! {
                    <Stack gap="xs">
                        {unit.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
                        {drawn}
                    </Stack>
                }
                .into_any(),
            );
        }

        let emit = context.emit.clone();

        let fraction = |event: &leptos::ev::PointerEvent| -> Option<f64> {
            let target = event.current_target()?;
            let element = target.dyn_into::<web_sys::Element>().ok()?;
            let box_ = element.get_bounding_client_rect();
            if box_.width() <= 0.0 {
                return None;
            }
            Some(((event.client_x() as f64 - box_.x()) / box_.width()).clamp(0.0, 1.0))
        };

        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    <Group gap="xs">
                        <Text size="xs" dimmed=true>"drag to choose a range"</Text>
                        {unit.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
                    </Group>
                    <div
                        class="hlin-brush"
                        on:pointerdown=move |event: leptos::ev::PointerEvent| {
                            BRUSH_START.with(|start| start.set(fraction(&event)));
                        }
                        on:pointerup=move |event: leptos::ev::PointerEvent| {
                            let Some(from) = BRUSH_START.with(|start| start.take()) else {
                                return;
                            };
                            let Some(to) = fraction(&event) else { return };

                            // A click is not a brush. Below a small fraction of
                            // the width this is somebody tapping the chart, and
                            // moving the whole surface for that would be a
                            // surprise rather than a feature.
                            let (low, high) = if from <= to { (from, to) } else { (to, from) };
                            if high - low < 0.02 {
                                return;
                            }

                            let span = (last - first) as f64;
                            emit.send(Intent::Range {
                                from_millis: first + (span * low) as i64,
                                to_millis: first + (span * high) as i64,
                            });
                        }
                    >
                        {drawn}
                    </div>
                </Stack>
            }
            .into_any(),
        )
    }

    /// `aurora.graph`: a records envelope as a directed graph.
    ///
    /// Hlin has no vocabulary for a graph and this does not give it one. What
    /// arrives is `records.v1` — the same rows any table draws — and the shape
    /// is read out of it here, by agreement between the platform and this pack:
    /// an `id`, a `label`, and a `depends_on` naming the row it flows from.
    ///
    /// A row naming a dependency nobody declared is dropped rather than
    /// inventing the node, which is what keeps a partial answer from a platform
    /// drawing a graph that misrepresents it.
    fn graph(data: &Records, context: Context) -> AnyView {
        let text = |row: &serde_json::Map<String, serde_json::Value>, key: &str| {
            row.get(key).and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.clone()),
                serde_json::Value::Null => None,
                other => Some(plain(other)),
            })
        };

        let nodes: Vec<GraphNode> = data
            .rows
            .iter()
            .filter_map(|row| {
                let id = text(row, "id")?;
                let label = text(row, "label").unwrap_or_else(|| id.clone());
                let node = GraphNode::new(id, label).color(match text(row, "state").as_deref() {
                    Some("degraded") => token::GOLD,
                    Some("down") => token::BAD,
                    _ => token::OK,
                });
                Some(match text(row, "role") {
                    Some(role) => node.sublabel(role),
                    None => node,
                })
            })
            .collect();

        let declared: std::collections::HashSet<&str> =
            nodes.iter().map(|node| node.id.as_str()).collect();

        let edges: Vec<GraphEdge> = data
            .rows
            .iter()
            .filter_map(|row| {
                let to = text(row, "id")?;
                let from = text(row, "depends_on")?;
                (declared.contains(from.as_str()) && declared.contains(to.as_str()))
                    .then(|| GraphEdge::new(from, to).active(true))
            })
            .collect();

        Self::framed(
            context,
            view! { <Graph nodes=nodes edges=edges direction="LR" /> }.into_any(),
        )
    }
}

impl DesignPack for AuroraPack {
    type View = AnyView;

    fn stat(&self, data: &Scalar, context: Context) -> Self::View {
        // Everything is pulled out of the envelope before the view, because
        // Aurora's components take their children as boxed closures and a
        // borrow captured in one would have to outlive this call.
        let shown = match &data.value {
            serde_json::Value::Null => "—".to_string(),
            other => plain(other),
        };
        let unit = data.unit.clone();
        let label = data.label.clone();

        // A platform sending a previous value is asking for a direction. Which
        // direction is good is not knowable here — more errors is worse, more
        // throughput is better — so this states the change and colours it by
        // sign, which is the most an envelope with no polarity supports.
        let delta = data
            .previous
            .zip(data.value.as_f64())
            .filter(|(previous, _)| *previous != 0.0)
            .map(|(previous, current)| {
                let change = (current - previous) / previous.abs() * 100.0;
                let colour = if change >= 0.0 { token::OK } else { token::BAD };
                let sign = if change >= 0.0 { "+" } else { "" };
                (format!("{sign}{change:.1}% vs previous"), colour)
            });

        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    <Group gap="xs" top=true>
                        <span class="hlin-stat">{shown}</span>
                        {unit.map(|text| view! { <Text size="sm" dimmed=true>{text}</Text> })}
                    </Group>
                    {label.map(|text| view! { <Text size="sm" dimmed=true>{text}</Text> })}
                    {delta.map(|(text, colour)| view! { <Pill color=colour>{text}</Pill> })}
                </Stack>
            }
            .into_any(),
        )
    }

    fn timeseries(&self, data: &Series, context: Context) -> Self::View {
        let unit = data.unit.clone();
        let plotted = plot(data);
        let legend: Vec<(String, &'static str)> = data
            .series
            .iter()
            .enumerate()
            .map(|(index, line)| (line.name.clone(), series_colour(index)))
            .collect();

        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    {unit.map(|text| view! { <Text size="xs" dimmed=true>{text}</Text> })}
                    {chart(plotted)}
                    <Group gap="sm" wrap=true>
                        {legend.into_iter().map(|(name, colour)| view! {
                            <Group gap="xs">
                                <Dot color=colour size=8 />
                                <Text size="xs" dimmed=true>{name}</Text>
                            </Group>
                        }).collect_view()}
                    </Group>
                </Stack>
            }
            .into_any(),
        )
    }

    fn sparkline(&self, data: &Series, context: Context) -> Self::View {
        let rows: Vec<(String, String, &'static str, String)> = data
            .series
            .iter()
            .zip(spark(data))
            .map(|(line, (path, colour))| {
                let latest = line
                    .points
                    .iter()
                    .rev()
                    .find_map(|point| point.1)
                    .map(trim)
                    .unwrap_or_else(|| "—".to_string());
                (line.name.clone(), path, colour, latest)
            })
            .collect();

        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    {rows.into_iter().map(|(name, path, colour, latest)| view! {
                        <Group justify="between" gap="sm">
                            <Text size="xs" dimmed=true>{name}</Text>
                            <svg class="hlin-spark" viewBox="0 0 240 28" preserveAspectRatio="none">
                                // Stretched to fill, which is what a sparkline
                                // is for — but the stroke is not, or it comes
                                // out thin where the line runs flat and thick
                                // where it runs steep. That is the whole of the
                                // ribboning the full chart had.
                                <path
                                    d=path
                                    fill="none"
                                    stroke=colour
                                    stroke-width="1.5"
                                    vector-effect="non-scaling-stroke"
                                    stroke-linejoin="round"
                                />
                            </svg>
                            <Text size="sm" bright=true mono=true>{latest}</Text>
                        </Group>
                    }).collect_view()}
                </Stack>
            }
            .into_any(),
        )
    }

    fn table(&self, data: &Records, context: Context) -> Self::View {
        let headings: Vec<String> = data
            .columns
            .iter()
            .map(|column| column.label.clone())
            .collect();
        let keys: Vec<String> = data
            .columns
            .iter()
            .map(|column| column.key.clone())
            .collect();

        // A key the row does not carry renders empty; a key no column declared
        // is ignored. Both are the envelope's rules rather than this pack's.
        let rows: Vec<Vec<String>> = data
            .rows
            .iter()
            .map(|row| {
                keys.iter()
                    .map(|key| row.get(key).map(plain).unwrap_or_default())
                    .collect()
            })
            .collect();

        Self::framed(context, grid(headings, rows))
    }

    fn series_as_table(&self, data: &Series, context: Context) -> Self::View {
        let mut headings = vec!["Time".to_string()];
        headings.extend(data.series.iter().map(|line| line.name.clone()));

        // Instants come from the first series. A platform sending series that
        // do not share a time base is sending something a table cannot honestly
        // lay out, and taking the first beats inventing a join.
        let instants: Vec<i64> = data
            .series
            .first()
            .map(|line| line.points.iter().map(|point| point.0).collect())
            .unwrap_or_default();

        let rows: Vec<Vec<String>> = instants
            .into_iter()
            .enumerate()
            .map(|(index, at)| {
                let mut row = vec![clock(at)];
                row.extend(data.series.iter().map(|line| {
                    line.points
                        .get(index)
                        .and_then(|point| point.1)
                        .map(trim)
                        .unwrap_or_default()
                }));
                row
            })
            .collect();

        Self::framed(context, grid(headings, rows))
    }

    fn status(&self, data: &Status, context: Context) -> Self::View {
        let rollup = health(data.status);
        let label = data
            .label
            .clone()
            .unwrap_or_else(|| word(data.status).to_string());
        let detail = data.detail.clone();
        let tip = detail.clone().unwrap_or_default();

        let parts: Vec<(String, &'static str, Option<String>)> = data
            .items
            .iter()
            .map(|item| (item.name.clone(), health(item.status), item.detail.clone()))
            .collect();

        Self::framed(
            context,
            view! {
                <Stack gap="sm">
                    <HealthPill label=label color=rollup tip=tip />
                    {detail.map(|text| view! { <Text size="sm" dimmed=true>{text}</Text> })}
                    <Stack gap="xs">
                        {parts.into_iter().map(|(name, colour, detail)| view! {
                            <Group gap="xs" justify="between">
                                <Group gap="xs">
                                    <Dot color=colour size=8 />
                                    <Text size="sm">{name}</Text>
                                </Group>
                                {detail.map(|text| view! {
                                    <Text size="xs" dimmed=true>{text}</Text>
                                })}
                            </Group>
                        }).collect_view()}
                    </Stack>
                </Stack>
            }
            .into_any(),
        )
    }

    fn options(&self, data: &Options, context: Context) -> Self::View {
        // Only `raw` accepts an options document, because it is read by a
        // control rather than shown as a panel. It still has to draw.
        let choices: Vec<String> = data
            .options
            .iter()
            .map(|choice| choice.label.clone())
            .collect();

        let body = if choices.is_empty() {
            view! { <Empty message="no options" /> }.into_any()
        } else {
            view! {
                <List>
                    {choices.into_iter().map(|label| view! { <ListItem>{label}</ListItem> })
                        .collect_view()}
                </List>
            }
            .into_any()
        };

        Self::framed(context, body)
    }

    fn raw(&self, data: &Envelope, context: Context) -> Self::View {
        let name = data.name().to_string();
        let document = serde_json::to_string_pretty(data)
            .unwrap_or_else(|_| "this document could not be shown".to_string());

        // Deliberately plain. Readable enough to be useful, unremarkable enough
        // that nobody ships it on purpose.
        Self::framed(
            context,
            view! {
                <Stack gap="xs">
                    <Code>{name}</Code>
                    <pre class="hlin-raw">{document}</pre>
                </Stack>
            }
            .into_any(),
        )
    }

    /// Aurora's own components, offered to platforms that ask for them by name.
    ///
    /// This is the half of the arrangement that runs the other way. Everywhere
    /// else in this file, Hlin names a kind and Aurora draws it. Here a
    /// platform names an Aurora component and Hlin passes the string through
    /// without knowing what it means, so a platform can reach a `Meter` or a
    /// `Graph` — things Hlin has no vocabulary for and no plans to grow one
    /// for.
    ///
    /// The names are Aurora's to choose and Aurora's to keep stable, which is
    /// why they are prefixed: a platform naming `aurora.meter` is asking this
    /// design system specifically, and a shell running a different one will
    /// draw the panel's declared kind instead.
    ///
    /// Anything not listed returns `None`, including a component that is real
    /// but whose data does not fit it. Declining on a shape mismatch matters:
    /// the alternative is drawing an empty `Graph`, and a panel that fell back
    /// to its declared table is more use than a component pretending it worked.
    fn offers(&self) -> &'static [&'static str] {
        &["aurora.meter", "aurora.graph", "aurora.faceted", "aurora.brush"]
    }

    fn custom(&self, component: &str, data: &Envelope, context: Context) -> Option<Self::View> {
        match (component, data) {
            ("aurora.meter", Envelope::Scalar(scalar)) => Some(Self::meter(scalar, context)),
            ("aurora.graph", Envelope::Records(records)) => Some(Self::graph(records, context)),
            ("aurora.faceted", Envelope::Series(series)) => Some(Self::faceted(series, context)),
            ("aurora.brush", Envelope::Series(series)) => Some(Self::brush(series, context)),
            _ => None,
        }
    }

    fn skeleton(&self, kind: Kind, _context: Context) -> Self::View {
        view! { <Loading label=format!("loading {kind}") /> }.into_any()
    }

    fn placeholder(&self, _kind: Kind, context: Context) -> Self::View {
        let Some(notice) = context.notice else {
            return view! { <Empty message="nothing to show" /> }.into_any();
        };

        // Forbidden is not a fault. Colouring it like one tells a person
        // something is broken when the answer is that this is not theirs.
        let colour = match notice.cause {
            Cause::Forbidden | Cause::Deprecated => token::GOLD,
            Cause::Unknown => token::MUTED,
            Cause::Unreachable | Cause::Malformed => token::BAD,
        };

        view! {
            <Alert title=heading(notice.cause) color=colour>
                <Text size="sm" dimmed=true>{explain(notice.cause)}</Text>
            </Alert>
        }
        .into_any()
    }

    fn stylesheet(&self) -> &'static str {
        HLIN_CSS
    }
}

// -- Meaning ---------------------------------------------------------------

/// A health value as an Aurora colour.
///
/// Aurora takes a colour and draws it. Deciding that degraded is gold is a
/// judgement about Hlin's vocabulary, so it is made here rather than in the
/// design system.
fn health(status: Health) -> &'static str {
    match status {
        Health::Ok => token::OK,
        Health::Degraded => token::GOLD,
        Health::Down => token::BAD,
        Health::Unknown => token::MUTED,
    }
}

fn word(status: Health) -> &'static str {
    match status {
        Health::Ok => "ok",
        Health::Degraded => "degraded",
        Health::Down => "down",
        Health::Unknown => "unknown",
    }
}

/// The colour of the nth series.
///
/// Aurora's accents in a fixed order, so one platform's chart looks the same on
/// every surface it appears on.
fn series_colour(index: usize) -> &'static str {
    const WHEEL: [&str; 6] = [
        token::ICE,
        token::TEAL,
        token::VIOLET,
        token::GOLD,
        token::OK,
        token::SKIP,
    ];
    WHEEL[index % WHEEL.len()]
}

/// The heading on a panel that cannot show anything.
///
/// Deliberately does not say who is at fault for `Unreachable`. A pack is handed
/// a cause and nothing else, and the unreachable party is sometimes a platform
/// and sometimes the shell itself; naming the wrong one sends a person to look
/// at a system that is answering perfectly well. Whoever knows writes it in the
/// detail beneath.
fn heading(cause: Cause) -> &'static str {
    match cause {
        Cause::Unreachable => "No answer",
        Cause::Malformed => "Unreadable",
        Cause::Unknown => "No longer offered",
        Cause::Deprecated => "Retired",
        Cause::Forbidden => "Not yours to see",
    }
}

fn explain(cause: Cause) -> &'static str {
    match cause {
        Cause::Unreachable => "nothing answered",
        Cause::Malformed => "what arrived could not be read",
        Cause::Unknown => "this panel no longer exists",
        Cause::Deprecated => "this panel has been retired",
        Cause::Forbidden => "you do not have access to this panel",
    }
}

// -- Numbers, time, tables -------------------------------------------------

fn plain(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .map(trim)
            .unwrap_or_else(|| number.to_string()),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(yes) => yes.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A number without a tail of zeroes nobody asked for.
fn trim(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// A millisecond instant as a clock time.
fn clock(at: i64) -> String {
    // Deliberately without a date library. A pack formats a timestamp for a
    // table cell; pulling `chrono` into a design system to do it would be a
    // dependency every consumer pays for one line.
    let seconds = at.div_euclid(1000).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// How long ago, in words.
fn ago(seconds: i64) -> String {
    match seconds {
        ..=59 => format!("{seconds}s old"),
        60..=3599 => format!("{}m old", seconds / 60),
        3600..=86_399 => format!("{}h old", seconds / 3600),
        _ => format!("{}d old", seconds / 86_400),
    }
}

/// Headings and rows as an Aurora table.
fn grid(headings: Vec<String>, rows: Vec<Vec<String>>) -> AnyView {
    view! {
        <Table mono=true>
            <thead>
                <tr>{headings.into_iter().map(|text| view! { <th>{text}</th> }).collect_view()}</tr>
            </thead>
            <tbody>
                {rows.into_iter().map(|row| view! {
                    <tr>{row.into_iter().map(|cell| view! { <td>{cell}</td> }).collect_view()}</tr>
                }).collect_view()}
            </tbody>
        </Table>
    }
    .into_any()
}

// -- Charts ----------------------------------------------------------------

/// The drawing area inside the chart, leaving room for the axes.
///
/// Nothing here is a proportion of the panel: the labels are text at a fixed
/// size, so the space they need is fixed too. A margin expressed as a
/// percentage would crowd them out on a narrow panel and strand them on a wide
/// one.
const CHART_W: f64 = 600.0;
const CHART_H: f64 = 150.0;
const PAD_LEFT: f64 = 52.0;
const PAD_RIGHT: f64 = 10.0;
const PAD_TOP: f64 = 12.0;
const PAD_BOTTOM: f64 = 26.0;

/// A series scaled to the drawing area, with the scale it was drawn against.
struct Plotted {
    lines: Vec<(String, &'static str)>,
    low: f64,
    high: f64,
    first: i64,
    last: i64,
    empty: bool,
}

/// Every series as an SVG path, and the scale they share.
///
/// Scaled together, because two series drawn to their own scales look
/// comparable and are not. A gap in the data is a gap in the path rather than a
/// straight line across it, so a platform that had nothing to report does not
/// appear to have reported a smooth number.
fn plot(data: &Series) -> Plotted {
    let width = CHART_W - PAD_LEFT - PAD_RIGHT;
    let height = CHART_H - PAD_TOP - PAD_BOTTOM;

    let values: Vec<f64> = data
        .series
        .iter()
        .flat_map(|line| line.points.iter().filter_map(|point| point.1))
        .collect();

    let (low, high) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(low, high), value| {
            (low.min(*value), high.max(*value))
        });
    let (low, high) = if values.is_empty() {
        (0.0, 1.0)
    } else if (high - low).abs() < f64::EPSILON {
        (low - 1.0, high + 1.0)
    } else {
        (low, high)
    };

    let (first, last) = data
        .series
        .iter()
        .flat_map(|line| line.points.iter().map(|point| point.0))
        .fold((i64::MAX, i64::MIN), |(first, last), at| {
            (first.min(at), last.max(at))
        });
    let span = ((last - first) as f64).max(1.0);

    let lines: Vec<(String, &'static str)> = data
        .series
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut path = String::new();
            let mut drawing = false;

            for point in &line.points {
                let Some(value) = point.1 else {
                    drawing = false;
                    continue;
                };
                let x = PAD_LEFT + (point.0 - first) as f64 / span * width;
                let y = PAD_TOP + height - (value - low) / (high - low) * height;

                if drawing {
                    path.push_str(&format!(" L{x:.1} {y:.1}"));
                } else {
                    path.push_str(&format!(" M{x:.1} {y:.1}"));
                    drawing = true;
                }
            }

            (path.trim().to_string(), series_colour(index))
        })
        .collect();

    let empty = lines.iter().all(|(path, _)| path.is_empty());
    Plotted {
        lines,
        low,
        high,
        first,
        last,
        empty,
    }
}

/// A number short enough to sit in an axis label.
///
/// Axis labels are read at a glance and compared to each other, so they are
/// rounded to something a person can hold in their head — full precision on a
/// tick is noise that makes the three of them harder to compare, not easier.
fn tick_label(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 10_000.0 {
        format!("{:.0}k", value / 1000.0)
    } else if magnitude >= 1000.0 {
        format!("{:.1}k", value / 1000.0)
    } else if magnitude >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// A wall-clock time from epoch milliseconds, without pulling in a date crate.
///
/// Only ever used for the two ends of the horizontal axis, where what matters
/// is telling one end from the other and knowing roughly when "now" is.
fn clock_label(at_millis: i64) -> String {
    let seconds = at_millis.div_euclid(1000);
    let day = seconds.rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        day / 3600,
        (day % 3600) / 60,
        day % 60
    )
}

/// The same shape, small enough to sit in a row.
///
/// Its own function rather than the chart's, because a sparkline has no axes to
/// leave room for: every unit of its box is drawing area, which is the point of
/// drawing one.
fn spark(data: &Series) -> Vec<(String, &'static str)> {
    const WIDTH: f64 = 240.0;
    const HEIGHT: f64 = 28.0;

    let plotted = plot(data);
    let inner_w = CHART_W - PAD_LEFT - PAD_RIGHT;
    let inner_h = CHART_H - PAD_TOP - PAD_BOTTOM;

    // Re-mapped from the chart's drawing area rather than re-scaled from the
    // data, so a sparkline and the chart beside it can never disagree about
    // where a value sits.
    plotted
        .lines
        .into_iter()
        .map(|(path, colour)| {
            let moved = path
                .split(' ')
                .map(|token| {
                    let Some((command, rest)) = token.split_at_checked(1) else {
                        return token.to_string();
                    };
                    if command != "M" && command != "L" {
                        return token.to_string();
                    }
                    let Ok(x) = rest.parse::<f64>() else {
                        return token.to_string();
                    };
                    format!("{command}{:.1}", (x - PAD_LEFT) / inner_w * WIDTH)
                })
                .collect::<Vec<_>>();

            // Only the x tokens carry a command; the y values follow them and
            // are rescaled in the same pass below.
            let rescaled = moved
                .into_iter()
                .map(|token| match token.parse::<f64>() {
                    Ok(y) => format!("{:.1}", (y - PAD_TOP) / inner_h * HEIGHT),
                    Err(_) => token,
                })
                .collect::<Vec<_>>()
                .join(" ");

            (rescaled, colour)
        })
        .collect()
}

/// The chart element itself.
///
/// Drawn to a fixed viewBox and scaled uniformly. The previous version stretched
/// a 240-unit box across whatever width the panel had with
/// `preserveAspectRatio="none"`, which scales x and y by different factors — so
/// a stroke came out thin where the line ran flat and thick where it ran steep,
/// and every series read as a ribbon rather than a line. It also put text in
/// that box, which stretched with it.
fn chart(plotted: Plotted) -> AnyView {
    if plotted.empty {
        return view! { <Empty message="no data in this range" /> }.into_any();
    }

    let left = PAD_LEFT;
    let right = CHART_W - PAD_RIGHT;
    let top = PAD_TOP;
    let bottom = CHART_H - PAD_BOTTOM;

    // Three horizontal rules: the low, the middle and the high of what is
    // actually drawn. Not a rounded scale of its own — a gridline that does not
    // touch the data invites reading a value off it that is not there.
    let ticks: Vec<(f64, String)> = [0.0_f64, 0.5, 1.0]
        .into_iter()
        .map(|fraction| {
            let value = plotted.low + (plotted.high - plotted.low) * fraction;
            let y = bottom - (bottom - top) * fraction;
            (y, tick_label(value))
        })
        .collect();

    view! {
        <svg class="hlin-chart" viewBox=format!("0 0 {CHART_W} {CHART_H}") role="img">
            {ticks.into_iter().map(|(y, label)| view! {
                <line
                    x1=left y1=y x2=right y2=y
                    class="hlin-axis__grid"
                />
                <text x=left - 8.0 y=y + 3.5 text-anchor="end" class="hlin-axis__label">
                    {label}
                </text>
            }).collect_view()}

            // The two axes themselves, drawn over the grid so a rule never sits
            // on top of the frame.
            <line x1=left y1=top x2=left y2=bottom class="hlin-axis__line" />
            <line x1=left y1=bottom x2=right y2=bottom class="hlin-axis__line" />

            <text x=left y=bottom + 16.0 text-anchor="start" class="hlin-axis__label">
                {clock_label(plotted.first)}
            </text>
            <text x=right y=bottom + 16.0 text-anchor="end" class="hlin-axis__label">
                {clock_label(plotted.last)}
            </text>
            {plotted.lines.into_iter().map(|(path, colour)| view! {
                <path
                    d=path
                    fill="none"
                    stroke=colour
                    stroke-width="1.5"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                />
            }).collect_view()}
        </svg>
    }
    .into_any()
}

use crate::components::Loading;
