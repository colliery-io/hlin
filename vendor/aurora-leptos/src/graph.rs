//! Generic graph / DAG drawing primitives.
//!
//! A node + edge model, a dependency-free **layered layout** (longest-path
//! layering — a Sugiyama-lite that looks good for typical DAGs), and an SVG
//! [`Graph`] component that renders Aurora-styled nodes and arrowed edges. For
//! large/complex graphs where crossing-minimisation matters, compute positions
//! with a dedicated layout crate (`layout-rs`/`rust-sugiyama`) and feed them in.
//!
//! The drawing is generic; apps supply node labels, accent colors, and which
//! edges are "active" (animated). Direction is top-to-bottom (`"TB"`, default)
//! or left-to-right (`"LR"`).

use std::collections::HashMap;

use leptos::prelude::*;

use crate::tokens::token;

/// A graph node. Use the builder methods for ergonomics.
#[derive(Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    /// Accent color (node border + arrow into it can stay neutral). App-supplied.
    pub color: String,
    /// Optional sub-label (e.g. a kind: "source", "sink").
    pub sublabel: Option<String>,
}

impl GraphNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), color: token::ICE.into(), sublabel: None }
    }
    pub fn color(mut self, c: impl Into<String>) -> Self {
        self.color = c.into();
        self
    }
    pub fn sublabel(mut self, s: impl Into<String>) -> Self {
        self.sublabel = Some(s.into());
        self
    }
}

/// A directed edge `from → to`. `active` animates it (pulse).
#[derive(Clone, PartialEq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub active: bool,
}

impl GraphEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self { from: from.into(), to: to.into(), active: false }
    }
    pub fn active(mut self, a: bool) -> Self {
        self.active = a;
        self
    }
}

/// A computed node position (center point).
pub struct NodePos {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Longest-path layered layout. Returns node centers + the canvas size.
/// `lr` lays out left-to-right; otherwise top-to-bottom.
pub fn layout_dag(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    lr: bool,
    node_w: f64,
    node_h: f64,
) -> (Vec<NodePos>, f64, f64) {
    let n = nodes.len();
    let idx: HashMap<&str, usize> =
        nodes.iter().enumerate().map(|(i, nd)| (nd.id.as_str(), i)).collect();
    let e: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|ed| Some((*idx.get(ed.from.as_str())?, *idx.get(ed.to.as_str())?)))
        .collect();

    // Longest-path layering (DAG): n relaxation passes converge.
    let mut layer = vec![0usize; n];
    for _ in 0..n {
        for &(f, t) in &e {
            if layer[t] < layer[f] + 1 {
                layer[t] = layer[f] + 1;
            }
        }
    }
    let num_layers = layer.iter().copied().max().map(|m| m + 1).unwrap_or(1);

    // Order within each layer by first appearance.
    let mut counts = vec![0usize; num_layers];
    let mut order = vec![0usize; n];
    for i in 0..n {
        let l = layer[i];
        order[i] = counts[l];
        counts[l] += 1;
    }
    let max_count = counts.iter().copied().max().unwrap_or(1).max(1) as f64;

    let m = 24.0;
    let along_gap = (if lr { node_h } else { node_w }) + 46.0; // siblings within a layer
    let layer_gap = (if lr { node_w } else { node_h }) + 64.0; // between layers
    let half_across = (if lr { node_w } else { node_h }) / 2.0;
    let along_span = max_count * along_gap;

    let mut pos = Vec::with_capacity(n);
    for i in 0..n {
        let l = layer[i] as f64;
        let cnt = counts[layer[i]] as f64;
        let o = order[i] as f64;
        let along = m + along_span / 2.0 + (o - (cnt - 1.0) / 2.0) * along_gap;
        let across = m + half_across + l * layer_gap;
        let (x, y) = if lr { (across, along) } else { (along, across) };
        pos.push(NodePos { id: nodes[i].id.clone(), x, y });
    }

    let across_dim = 2.0 * m + 2.0 * half_across + (num_layers as f64 - 1.0) * layer_gap;
    let along_dim = along_span + 2.0 * m;
    let (w, h) = if lr { (across_dim, along_dim) } else { (along_dim, across_dim) };
    (pos, w, h)
}

/// Renders `nodes` + `edges` as an auto-laid-out SVG DAG.
#[component]
pub fn Graph(
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    /// "TB" (top-to-bottom, default) or "LR" (left-to-right).
    #[prop(optional, into)]
    direction: String,
    #[prop(default = 150.0)] node_w: f64,
    #[prop(default = 48.0)] node_h: f64,
) -> impl IntoView {
    let lr = direction.eq_ignore_ascii_case("LR");
    let (pos, w, h) = layout_dag(&nodes, &edges, lr, node_w, node_h);
    let map: HashMap<String, (f64, f64)> =
        pos.into_iter().map(|p| (p.id, (p.x, p.y))).collect();

    // Edges: a curved stroke + a fixed-orientation arrowhead at the target face.
    let edge_views = edges
        .iter()
        .filter_map(|ed| {
            let (fx, fy) = *map.get(&ed.from)?;
            let (tx, ty) = *map.get(&ed.to)?;
            let stroke_class = if ed.active {
                "cl-graph__edge cl-graph__edge--active cl-pulse"
            } else {
                "cl-graph__edge"
            };
            let arrow_class = if ed.active {
                "cl-graph__arrow cl-graph__arrow--active cl-pulse"
            } else {
                "cl-graph__arrow"
            };
            let (d, arrow) = if lr {
                let (sx, sy, ex, ey) = (fx + node_w / 2.0, fy, tx - node_w / 2.0, ty);
                let k = ((ex - sx) * 0.4).max(18.0);
                (
                    format!("M{sx},{sy} C{},{sy} {},{ey} {ex},{ey}", sx + k, ex - k),
                    format!("M{},{} L{ex},{ey} L{},{} Z", ex - 8.0, ey - 4.5, ex - 8.0, ey + 4.5),
                )
            } else {
                let (sx, sy, ex, ey) = (fx, fy + node_h / 2.0, tx, ty - node_h / 2.0);
                let k = ((ey - sy) * 0.4).max(18.0);
                (
                    format!("M{sx},{sy} C{sx},{} {ex},{} {ex},{ey}", sy + k, ey - k),
                    format!("M{},{} L{ex},{ey} L{},{} Z", ex - 4.5, ey - 8.0, ex + 4.5, ey - 8.0),
                )
            };
            Some(view! {
                <path class=stroke_class d=d />
                <path class=arrow_class d=arrow />
            })
        })
        .collect_view();

    let node_views = nodes
        .iter()
        .map(|nd| {
            let (cx, cy) = map.get(&nd.id).copied().unwrap_or((0.0, 0.0));
            let x = cx - node_w / 2.0;
            let y = cy - node_h / 2.0;
            let has_sub = nd.sublabel.is_some();
            let label_y = if has_sub { cy - 5.0 } else { cy };
            view! {
                <g>
                    <rect
                        class="cl-graph__node"
                        x=x y=y width=node_w height=node_h rx="9"
                        style=format!("stroke:{};", nd.color)
                    />
                    <text class="cl-graph__label" x=cx y=label_y text-anchor="middle" dominant-baseline="middle">
                        {nd.label.clone()}
                    </text>
                    {nd.sublabel.clone().map(|s| view! {
                        <text class="cl-graph__sublabel" x=cx y=cy + 11.0 text-anchor="middle" dominant-baseline="middle">{s}</text>
                    })}
                </g>
            }
        })
        .collect_view();

    view! {
        <svg class="cl-graph" width=w height=h viewBox=format!("0 0 {w} {h}")>
            {edge_views}
            {node_views}
        </svg>
    }
}
