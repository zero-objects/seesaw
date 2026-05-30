//! DOT renderer for `TypedGraph` and `Cascade`.

use crate::engine::Cascade;
use crate::graph::{Status, TypedGraph};

#[derive(Clone, Debug)]
pub struct DotOpts {
    pub font: String,
    pub rankdir: Rankdir,
    pub show_ghost_ids: bool,
    pub hide_ghosts: bool,
}

impl Default for DotOpts {
    fn default() -> Self {
        Self {
            font: "monospace".into(),
            rankdir: Rankdir::TB,
            show_ghost_ids: false,
            hide_ghosts: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rankdir {
    TB,
    LR,
}

impl Rankdir {
    fn as_str(self) -> &'static str {
        match self {
            Rankdir::TB => "TB",
            Rankdir::LR => "LR",
        }
    }
}

pub fn typed_graph_to_dot(graph: &TypedGraph, opts: &DotOpts) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "digraph g {{\n    \
         fontname = \"{}\";\n    \
         rankdir = \"{}\";\n    \
         node [fontname = \"{}\"];\n    \
         edge [fontname = \"{}\"];\n",
        opts.font,
        opts.rankdir.as_str(),
        opts.font,
        opts.font,
    ));
    for node in graph.iter_nodes() {
        if opts.hide_ghosts && node.status == Status::Ghost {
            continue;
        }
        let label = format_node_label(node, opts);
        let style = status_node_style(node.status);
        s.push_str(&format!(
            "    \"{}\" [shape=box, label=\"{}\", {}];\n",
            short_id(&node.id),
            label,
            style,
        ));
    }
    for (src, tgt, edge) in graph.iter_edges() {
        let style = status_edge_style(edge.status);
        s.push_str(&format!(
            "    \"{}\" -> \"{}\" [label=\"{}\", {}];\n",
            short_id(&src),
            short_id(&tgt),
            edge.type_id,
            style,
        ));
    }
    s.push_str("}\n");
    s
}

fn short_id(id: &crate::graph::GhostId) -> String {
    id.short()
}

fn format_node_label(node: &crate::graph::NodeData, opts: &DotOpts) -> String {
    let mut lines = vec![node.type_id.clone()];
    if opts.show_ghost_ids {
        lines.push(node.id.short());
    }
    for (k, v) in &node.attrs {
        lines.push(format!("{k}={v}"));
    }
    lines.join("\\n")
}

fn status_node_style(s: Status) -> &'static str {
    match s {
        Status::Solid => "style=solid, color=\"black\"",
        Status::Ghost => "style=dashed, color=\"gray50\"",
        Status::TentativeTombstone => {
            "style=\"dashed,filled\", fillcolor=\"orange\", color=\"orange3\""
        }
        Status::Tombstone => "style=filled, fillcolor=\"mistyrose\", color=\"red\"",
    }
}

fn status_edge_style(s: Status) -> &'static str {
    match s {
        Status::Solid => "style=solid, color=\"black\"",
        Status::Ghost => "style=dashed, color=\"gray50\"",
        Status::TentativeTombstone => "style=\"dashed,bold\", color=\"orange\"",
        Status::Tombstone => "style=\"dashed,bold\", color=\"red\"",
    }
}

pub fn empty_graph_dot(annotation: &str, opts: &DotOpts) -> String {
    format!(
        "digraph g {{\n    \
         label = \"{}\";\n    \
         labelloc = \"t\";\n    \
         fontsize = 14;\n    \
         fontname = \"{}\";\n    \
         rankdir = \"{}\";\n\
         }}\n",
        annotation.replace('"', "\\\""),
        opts.font,
        opts.rankdir.as_str(),
    )
}

pub fn cascade_trace_to_dot(_pre: &TypedGraph, cascade: &Cascade) -> String {
    let mut s = String::new();
    s.push_str("digraph trace {\n    rankdir = \"LR\";\n    node [shape=box];\n");
    for (i, entry) in cascade.entries.iter().enumerate() {
        let origin = match &entry.origin {
            crate::ops::Origin::User => "User".to_string(),
            crate::ops::Origin::Rule { rule_id } => rule_id.clone(),
        };
        let ops_summary = entry
            .op_star
            .iter()
            .map(op_summary)
            .collect::<Vec<_>>()
            .join("\\n");
        s.push_str(&format!(
            "    step_{i} [label=\"{origin}\\n{ops_summary}\"];\n"
        ));
        if i > 0 {
            s.push_str(&format!("    step_{} -> step_{i};\n", i - 1));
        }
    }
    s.push_str("}\n");
    s
}

fn op_summary(op: &crate::ops::Op) -> String {
    match op {
        crate::ops::Op::AddNode { type_id, .. } => format!("AddNode({type_id})"),
        crate::ops::Op::AddEdge { type_id, .. } => format!("AddEdge({type_id})"),
        crate::ops::Op::DelNode { .. } => "DelNode".into(),
        crate::ops::Op::DelEdge { .. } => "DelEdge".into(),
        crate::ops::Op::SetAttr { key, value, .. } => format!("SetAttr({key}={value})"),
    }
}

#[cfg(feature = "regen_graphs")]
pub fn write_snapshot_triple(
    out_dir: &std::path::Path,
    graph: &TypedGraph,
    opts: &DotOpts,
) -> std::io::Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let source = typed_graph_to_dot(graph, opts);
    std::fs::write(out_dir.join("source.dot"), source)?;
    let target = empty_graph_dot("n/a — Mono-Graph-Case (STTT2021 hat kein R-Modell)", opts);
    std::fs::write(out_dir.join("target.dot"), target)?;
    let corr = empty_graph_dot("n/a — Mono-Graph-Case (keine Corr-Relation)", opts);
    std::fs::write(out_dir.join("corr.dot"), corr)?;
    Ok(())
}

#[cfg(test)]
#[path = "dot_tests.rs"]
mod dot_tests;
