use crate::diff::Summary;
use crate::graph::{Edge, Graph, Node};
use serde::Serialize;

#[derive(Serialize)]
struct Output<'a> {
    nodes: Vec<&'a Node>,
    edges: &'a [Edge],
    roots: Vec<&'a str>,
    unused: Vec<&'a str>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    diff: Option<&'a Summary>,
}

pub fn render(g: &Graph, summary: Option<&Summary>) -> String {
    let out = Output {
        nodes: g.nodes.values().collect(),
        edges: &g.edges,
        roots: g.roots(),
        unused: g.unused().iter().map(|n| n.name.as_str()).collect(),
        diff: summary,
    };
    serde_json::to_string_pretty(&out).unwrap()
}

pub fn render_nested(g: &Graph, root: Option<&str>, inside: Option<&str>) -> String {
    let value = if let Some(target) = inside {
        let list: Vec<_> = g
            .inside_of(target)
            .into_iter()
            .map(|(caller, slots)| serde_json::json!({"caller": caller, "slots": slots}))
            .collect();
        serde_json::json!({"inside": target, "callers": list})
    } else if let Some(root) = root {
        serde_json::json!({"root": root, "slots": g.nested.get(root)})
    } else {
        serde_json::json!({"roots": g.roots(), "nested": g.nested})
    };
    serde_json::to_string_pretty(&value).unwrap()
}
