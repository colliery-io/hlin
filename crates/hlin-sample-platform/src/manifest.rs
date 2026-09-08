//! The manifest this platform serves about itself.
//!
//! Built as a value rather than a string so the binary can validate it with
//! `hlin-manifest` before serving it. A reference that is wrong is worse than
//! no reference at all, so the platform refuses to start if its own manifest
//! would be rejected.

use hlin_manifest::manifest::{
    Lifecycle, Manifest, NavigationEntry, Panel, ParamDecl, Platform, SUPPORTED_SCHEMA_VERSION,
};
use serde_json::{Map, Value};

/// The contract version this platform declares when it is behaving.
///
/// 2.1.0 adds `stage-activity`; adding a panel is additive, so it is a minor.
/// 2.0.0 because `throughput-by-cluster` stopped declaring `time_range`, and
/// `params` is contract: a consumer that pinned to the panel responding to the
/// time picker would break. The shell caught this before anybody noticed —
/// `angreal demo walkthrough` failed with the violation naming the exact
/// parameter — which is what HLIN-A-0002 exists to do, working against the code
/// that was demonstrating it.
pub const NORMAL_VERSION: &str = "2.1.0";

/// The panel key dropped by `--breaking`, without a major bump.
pub const BREAKING_PANEL_KEY: &str = "queue-depth";

/// Build the manifest for a platform of this name.
///
/// `breaking` drops one panel while leaving `contract_version` alone, which is
/// exactly the mistake the shell's runtime enforcement exists to catch
/// (decision HLIN-A-0002). It is a flag rather than a separate binary so the
/// walkthrough can restart one platform into the misbehaving state.
pub fn build(name: &str, breaking: bool) -> Manifest {
    let panels = if breaking {
        all_panels(name)
            .into_iter()
            .filter(|panel| panel.key != BREAKING_PANEL_KEY)
            .collect()
    } else {
        all_panels(name)
    };

    Manifest {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        contract_version: NORMAL_VERSION.parse().expect("a valid semver constant"),
        platform: Platform {
            id: name.to_string(),
            name: title_case(name),
            icon: Some("database".to_string()),
            extra: Default::default(),
        },
        navigation: vec![NavigationEntry {
            label: "Overview".to_string(),
            path: "overview".to_string(),
            icon: Some("inbox".to_string()),
            weight: 10,
            extra: Default::default(),
        }],
        panels,
        health: "api/health".to_string(),
        events: Some("api/events".to_string()),
        extra: Default::default(),
    }
}

/// Every panel this platform offers.
///
/// The set is chosen to exercise the whole vocabulary and one shell behaviour
/// that is otherwise invisible: `throughput` and `throughput-compact` read the
/// *same* endpoint with the same parameters, so a surface holding both must
/// produce one upstream request rather than two.
fn all_panels(name: &str) -> Vec<Panel> {
    vec![
        // An ingest rate moves second to second, so it says so. Four times a
        // second is not the shell showing off: it is what the number does.
        every(
            panel(
                "records-per-second",
                "Records per second",
                Some("Current ingest rate across all workers"),
                "stat",
                "scalar.v1",
                "api/hlin/records-per-second",
                vec![],
            ),
            250,
        ),
        panel(
            "throughput",
            "Throughput",
            Some("Ingest rate per worker over time"),
            "timeseries",
            "series.v1",
            "api/hlin/throughput",
            vec![ParamDecl::bare("time_range")],
        ),
        panel(
            "throughput-compact",
            "Throughput at a glance",
            Some("The same data as Throughput, drawn small"),
            "sparkline",
            "series.v1",
            "api/hlin/throughput",
            vec![ParamDecl::bare("time_range")],
        ),
        // A queue moves while you watch it. Half a second is enough to see it
        // move without asking for more than the data actually says.
        every(
            panel(
                "queue-depth",
                "Queue depth by stage",
                Some("How much work is waiting, and where"),
                "table",
                "records.v1",
                "api/hlin/queue-depth",
                vec![],
            ),
            500,
        ),
        panel(
            "worker-health",
            "Worker health",
            Some("One row per worker, with the rollup"),
            "status",
            "status.v1",
            "api/hlin/worker-health",
            vec![],
        ),
        // The two panels whose data genuinely moves on a scale of seconds say
        // so, so a shell on its ordinary half-minute interval still draws them
        // live. Before this, seeing them move needed a whole second demo
        // configuration.
        every(
            panel(
                "live-rate",
                "Live rate",
                Some("Events per second, moving fast enough to watch"),
                "stat",
                "scalar.v1",
                "api/hlin/live-rate",
                vec![],
            ),
            125,
        ),
        every(
            panel(
                "live-throughput",
                "Live throughput",
                Some("The last minute at quarter-second resolution"),
                "timeseries",
                "series.v1",
                "api/hlin/live-throughput",
                vec![],
            ),
            125,
        ),
        // The only panel here whose data this platform actually keeps rather
        // than derives, and therefore the only one it can honestly report on.
        //
        // It declares a fast cadence *and* offers to be pushed, which is the
        // combination the feature exists for: without the stream a shell has to
        // ask four times a second to make a count that moves every few seconds
        // feel immediate. With it, the shell polls at the relaxed interval and
        // hears about the change as it happens — better latency for a fraction
        // of the requests. A panel whose data genuinely moves eight times a
        // second, like the two above, gains nothing from being pushed and does
        // not claim to be.
        reports_its_own(every(
            panel(
                crate::changes::BATCHES,
                "Batches completed",
                Some("A count that moves when it moves, not on a schedule"),
                "stat",
                "scalar.v1",
                "api/hlin/batches",
                vec![],
            ),
            250,
        )),
        // The drill-down. The graph says how a stage *is*; this says what it
        // has been doing, which is the question somebody asks the moment they
        // see a node go red — and the one no amount of staring at the graph
        // answers.
        //
        // Its `select` is what makes it a drill-down rather than a second
        // table: a click on a stage sets the parameter, the shell refetches,
        // and the list is that stage's. The same parameter the shell's own
        // control writes to, so a person can drive it either way.
        drawn_by(
            reports_its_own(panel(
                crate::changes::ACTIVITY,
                "Stage activity",
                Some("Pick a stage; see what has just happened to it"),
                "table",
                "records.v1",
                "api/hlin/stage-activity",
                vec![select("stage", "Stage", &format!("api/hlin/{name}-stages"))],
            )),
            "aurora.drilldown",
        ),
        // Two panels that name a component as well as a kind. Both are drawn by
        // Aurora's own components where a shell mounts Aurora, and by the
        // declared kind everywhere else. A platform can do this without knowing
        // which design system any shell it registers against is running.
        drawn_by(
            every(
                panel(
                    "saturation",
                    "Capacity",
                    Some("How full this platform is, as a proportion"),
                    "stat",
                    "scalar.v1",
                    "api/hlin/saturation",
                    vec![],
                ),
                500,
            ),
            "aurora.meter",
        ),
        // The shape does not change; the health does, and that is the half a
        // DAG on a wall is watched for.
        //
        // The best case for push in this whole platform. A stage going down is
        // a discrete event somebody needs to see *now* and which happens
        // rarely — so polling it forces the choice the feature exists to
        // remove. Declared at half a second, which is what it would need to
        // feel immediate without a stream; with one, that becomes the relaxed
        // interval's two requests a minute and the change still arrives at
        // once.
        drawn_by(
            reports_its_own(every(
                panel(
                    crate::changes::PIPELINE,
                    "Pipeline",
                    Some("The stages, what they read from, and how they are"),
                    "table",
                    "records.v1",
                    "api/hlin/pipeline",
                    vec![],
                ),
                500,
            )),
            "aurora.graph",
        ),
        // No `time_range`: this one answers the last minute, live, so a person
        // picking a cluster sees the answer change while they watch. The time
        // picker is demonstrated by `throughput` and `throughput-compact`,
        // which are genuinely about a chosen range.
        every(
            panel(
                "throughput-by-cluster",
                "Throughput by cluster",
                Some("Ingest rate for one cluster; pick which"),
                "timeseries",
                "series.v1",
                "api/hlin/throughput-by-cluster",
                vec![select(
                    "cluster",
                    "Cluster",
                    &format!("api/hlin/{name}-clusters"),
                )],
            ),
            250,
        ),
        drawn_by(
            panel(
                "throughput-brush",
                "Throughput, drag to zoom",
                Some("Drag across it to move the whole surface to that window"),
                "timeseries",
                "series.v1",
                "api/hlin/throughput",
                vec![ParamDecl::bare("time_range")],
            ),
            "aurora.brush",
        ),
        // The same panel and the same data, asking a design system to draw
        // the filter as well as the chart. Where the pack has no such
        // component the chart is drawn and the shell's own control does the
        // filtering, which is what every other panel here gets.
        drawn_by(
            panel(
                "throughput-faceted",
                "Throughput, filtered in place",
                Some("Pick a cluster without leaving the panel"),
                "timeseries",
                "series.v1",
                "api/hlin/throughput-by-cluster",
                vec![
                    ParamDecl::bare("time_range"),
                    select("cluster", "Cluster", &format!("api/hlin/{name}-clusters")),
                ],
            ),
            "aurora.faceted",
        ),
    ]
}

fn panel(
    key: &str,
    title: &str,
    description: Option<&str>,
    kind: &str,
    envelope: &str,
    data: &str,
    params: Vec<ParamDecl>,
) -> Panel {
    Panel {
        key: key.to_string(),
        title: title.to_string(),
        description: description.map(str::to_string),
        kind: kind.to_string(),
        envelope: envelope.to_string(),
        data: data.to_string(),
        params,
        refresh_ms: None,
        pushed: false,
        component: None,
        lifecycle: Lifecycle::default(),
        extra: Default::default(),
    }
}

/// Say how often this panel's data is worth refetching.
///
/// A hint the shell clamps (decision HLIN-A-0009), not an instruction. Declared
/// only by the panels whose data actually moves on a scale of seconds — the
/// rest are content with whatever the shell's own interval is, which is what
/// makes this worth having: one number cannot suit both.
fn every(panel: Panel, refresh_ms: u64) -> Panel {
    Panel {
        refresh_ms: Some(refresh_ms),
        ..panel
    }
}

/// Ask for a component from the design system by name.
///
/// The platform and the design pack agree on this string; the shell forwards it
/// without knowing what it means. Written as a wrapper rather than another
/// argument to `panel` because most panels want nothing to do with it, and an
/// eighth positional `None` on every one of them would say so less clearly.
fn drawn_by(panel: Panel, component: &str) -> Panel {
    Panel {
        component: Some(component.to_string()),
        ..panel
    }
}

fn select(id: &str, label: &str, options: &str) -> ParamDecl {
    let mut config = Map::new();
    config.insert("id".to_string(), Value::String(id.to_string()));
    config.insert("label".to_string(), Value::String(label.to_string()));
    config.insert("options".to_string(), Value::String(options.to_string()));
    ParamDecl {
        param: "select".to_string(),
        config,
    }
}

fn title_case(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Offer to tell the shell when this panel's data changes.
///
/// Separate from the platform-level `events` path on purpose: a platform that
/// serves a stream still has to say which of its panels it actually reports on,
/// or the shell cannot tell silence from having nothing to say and would poll a
/// panel less on a promise nobody made about it (specification HLIN-S-0006).
fn reports_its_own(panel: Panel) -> Panel {
    Panel {
        pushed: true,
        ..panel
    }
}
