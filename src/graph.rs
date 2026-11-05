use crate::scan::{FileScan, Via};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Component,
    View,
    Layout,
    Entry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub via: Via,
    pub file: String,
    pub line: usize,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub name: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inside: Vec<Slot>,
}

#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: BTreeMap<String, Node>,
    pub edges: Vec<Edge>,
    pub nested: BTreeMap<String, Vec<Slot>>,
    out: HashMap<String, Vec<usize>>,
    inc: HashMap<String, Vec<usize>>,
}

pub fn is_phlex_file(path: &str) -> bool {
    let in_dir = path.contains("app/components/") || path.contains("app/views/");
    in_dir && path.ends_with(".rb")
}

fn kind_for(name: &str) -> Kind {
    if name.contains("Layout") {
        Kind::Layout
    } else if name.starts_with("Views::") {
        Kind::View
    } else {
        Kind::Component
    }
}

fn resolve(
    written: &str,
    scope: &[String],
    known: &BTreeMap<String, Node>,
    in_phlex: bool,
) -> Option<String> {
    if let Some(abs) = written.strip_prefix("::") {
        return known.contains_key(abs).then(|| abs.to_string());
    }
    let lexical = (0..=scope.len()).rev().map(|i| {
        let mut parts = scope[..i].to_vec();
        parts.push(written.to_string());
        parts.join("::")
    });
    lexical
        .chain(in_phlex.then(|| format!("Components::{written}")))
        .find(|c| known.contains_key(c))
}

impl Graph {
    pub fn build(scans: &[FileScan]) -> Graph {
        let mut nodes = BTreeMap::new();
        for s in scans.iter().filter(|s| is_phlex_file(&s.path)) {
            for c in &s.classes {
                nodes.entry(c.name.clone()).or_insert_with(|| Node {
                    name: c.name.clone(),
                    file: s.path.clone(),
                    line: c.line,
                    kind: kind_for(&c.name),
                    parent: None,
                    status: None,
                });
            }
        }

        let mut parents = Vec::new();
        for s in scans.iter().filter(|s| is_phlex_file(&s.path)) {
            for c in &s.classes {
                let parent = c
                    .parent
                    .as_deref()
                    .and_then(|p| resolve(p, &c.scope, &nodes, true));
                parents.push((c.name.clone(), parent));
            }
        }
        for (name, parent) in parents {
            if let Some(n) = nodes.get_mut(&name) {
                n.parent = n.parent.take().or(parent);
            }
        }

        let mut edges: BTreeMap<(String, String), Edge> = BTreeMap::new();
        let mut entries = Vec::new();
        let mut nested: BTreeMap<String, Vec<Slot>> = BTreeMap::new();
        for s in scans {
            let phlex = is_phlex_file(&s.path);
            let resolved: Vec<Option<String>> = s
                .calls
                .iter()
                .map(|c| resolve(&c.name, &c.scope, &nodes, phlex))
                .collect();
            for (caller, slots) in nest(s, &resolved) {
                nested.entry(caller).or_default().extend(slots);
            }
            for (call, to) in s.calls.iter().zip(resolved) {
                let Some(to) = to else {
                    continue;
                };
                let from = call.caller.clone().unwrap_or_else(|| s.path.clone());
                let from_known = phlex && nodes.contains_key(&from);
                if !from_known {
                    entries.push(Node {
                        name: from.clone(),
                        file: s.path.clone(),
                        line: 1,
                        kind: Kind::Entry,
                        parent: None,
                        status: None,
                    });
                }
                edges
                    .entry((from.clone(), to.clone()))
                    .and_modify(|e| e.count += 1)
                    .or_insert(Edge {
                        from,
                        to,
                        via: call.via,
                        file: s.path.clone(),
                        line: call.line,
                        count: 1,
                        status: None,
                    });
            }
        }
        for e in entries {
            nodes.entry(e.name.clone()).or_insert(e);
        }

        let mut g = Graph::from_parts(nodes, edges.into_values().collect());
        g.nested = nested.into_iter().map(|(k, v)| (k, merge(v))).collect();
        g
    }

    pub fn inside_of(&self, target: &str) -> Vec<(&str, Vec<Slot>)> {
        self.nested
            .iter()
            .filter_map(|(caller, slots)| {
                let mut found = Vec::new();
                collect_inside(slots, target, &mut found);
                (!found.is_empty()).then(|| (caller.as_str(), merge(found)))
            })
            .collect()
    }

    pub fn from_parts(nodes: BTreeMap<String, Node>, edges: Vec<Edge>) -> Graph {
        let mut g = Graph {
            nodes,
            edges,
            ..Default::default()
        };
        for (i, e) in g.edges.iter().enumerate() {
            g.out.entry(e.from.clone()).or_default().push(i);
            g.inc.entry(e.to.clone()).or_default().push(i);
        }
        g
    }

    pub fn children(&self, name: &str) -> Vec<&Edge> {
        self.pick(self.out.get(name))
    }

    pub fn callers(&self, name: &str) -> Vec<&Edge> {
        self.pick(self.inc.get(name))
    }

    fn pick(&self, ids: Option<&Vec<usize>>) -> Vec<&Edge> {
        ids.map(|v| v.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    pub fn roots(&self) -> Vec<&str> {
        let mut roots: Vec<&Node> = self
            .nodes
            .values()
            .filter(|n| self.callers(&n.name).is_empty())
            .filter(|n| !self.children(&n.name).is_empty())
            .collect();
        roots.sort_by_key(|n| (n.kind != Kind::Entry, n.name.clone()));
        roots.into_iter().map(|n| n.name.as_str()).collect()
    }

    pub fn base_classes(&self) -> BTreeSet<&str> {
        self.nodes
            .values()
            .filter_map(|n| n.parent.as_deref())
            .collect()
    }

    pub fn unused(&self) -> Vec<&Node> {
        let bases = self.base_classes();
        self.nodes
            .values()
            .filter(|n| n.kind == Kind::Component)
            .filter(|n| !bases.contains(n.name.as_str()))
            .filter(|n| self.callers(&n.name).is_empty())
            .collect()
    }
}

fn nest(s: &FileScan, resolved: &[Option<String>]) -> BTreeMap<String, Vec<Slot>> {
    let is_method = |i: usize| s.calls[i].via == Via::Method;
    let counts = |i: usize| resolved[i].is_some() || is_method(i);
    let owner = |mut i: usize| loop {
        match s.calls[i].parent {
            Some(p) if resolved[p].is_some() => return Some(p),
            Some(p) => i = p,
            None => return None,
        }
    };

    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut tops: BTreeMap<(String, Option<String>), Vec<usize>> = BTreeMap::new();
    let mut called: HashMap<String, BTreeSet<String>> = HashMap::new();
    for i in (0..s.calls.len()).filter(|&i| counts(i)) {
        let call = &s.calls[i];
        let caller = call.caller.clone().unwrap_or_else(|| s.path.clone());
        if is_method(i) {
            called.entry(caller.clone()).or_default().insert(call.name.clone());
        }
        match owner(i) {
            Some(p) => children.entry(p).or_default().push(i),
            None => tops.entry((caller, call.method.clone())).or_default().push(i),
        }
    }

    struct Ctx<'a> {
        s: &'a FileScan,
        resolved: &'a [Option<String>],
        children: HashMap<usize, Vec<usize>>,
        tops: BTreeMap<(String, Option<String>), Vec<usize>>,
    }
    fn build(ctx: &Ctx, i: usize, caller: &str, path: &mut Vec<String>) -> Vec<Slot> {
        let call = &ctx.s.calls[i];
        if call.via == Via::Method {
            if path.contains(&call.name) {
                return Vec::new();
            }
            path.push(call.name.clone());
            let key = (caller.to_string(), Some(call.name.clone()));
            let ids = ctx.tops.get(&key).cloned().unwrap_or_default();
            let slots = ids.iter().flat_map(|&c| build(ctx, c, caller, path)).collect();
            path.pop();
            return slots;
        }
        let inside = ctx
            .children
            .get(&i)
            .map(|v| v.iter().flat_map(|&c| build(ctx, c, caller, path)).collect())
            .unwrap_or_default();
        vec![Slot {
            name: ctx.resolved[i].clone().unwrap(),
            count: 1,
            inside,
        }]
    }

    let ctx = Ctx { s, resolved, children, tops };
    let mut out: BTreeMap<String, Vec<Slot>> = BTreeMap::new();
    for ((caller, method), ids) in &ctx.tops {
        let helper = method
            .as_ref()
            .is_some_and(|m| called.get(caller).is_some_and(|c| c.contains(m)));
        if helper {
            continue;
        }
        let mut path: Vec<String> = method.iter().cloned().collect();
        let slots: Vec<Slot> = ids
            .iter()
            .flat_map(|&i| build(&ctx, i, caller, &mut path))
            .collect();
        if !slots.is_empty() {
            out.entry(caller.clone()).or_default().extend(slots);
        }
    }
    out
}

pub fn merge(slots: Vec<Slot>) -> Vec<Slot> {
    let mut merged: Vec<Slot> = Vec::new();
    for slot in slots {
        match merged.iter_mut().find(|m| m.name == slot.name) {
            Some(m) => {
                m.count += slot.count;
                m.inside.extend(slot.inside);
            }
            None => merged.push(slot),
        }
    }
    for m in &mut merged {
        m.inside = merge(std::mem::take(&mut m.inside));
    }
    merged
}

fn collect_inside(slots: &[Slot], target: &str, found: &mut Vec<Slot>) {
    for slot in slots {
        if slot.name == target {
            found.push(slot.clone());
        }
        collect_inside(&slot.inside, target, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn graph(files: &[(&str, &str)]) -> Graph {
        let scans: Vec<_> = files.iter().map(|(p, s)| scan(p, s)).collect();
        Graph::build(&scans)
    }

    fn targets(g: &Graph, from: &str) -> Vec<String> {
        g.children(from).iter().map(|e| e.to.clone()).collect()
    }

    #[test]
    fn shorthand_resolves_to_components() {
        let g = graph(&[
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/views/wins/show.rb", "class Views::Wins::Show < Views::Base\n  def view_template\n    Win(win: 1)\n  end\nend\n"),
        ]);
        assert_eq!(targets(&g, "Views::Wins::Show"), vec!["Components::Win"]);
    }

    #[test]
    fn lexical_lookup_beats_components() {
        let g = graph(&[
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/views/wins/win.rb", "class Views::Wins::Win < Views::Base\nend\n"),
            ("app/views/wins/show.rb", "module Views\n  module Wins\n    class Show < Views::Base\n      def view_template\n        Win(win: 1)\n      end\n    end\n  end\nend\n"),
        ]);
        assert_eq!(targets(&g, "Views::Wins::Show"), vec!["Views::Wins::Win"]);
    }

    #[test]
    fn unknown_names_are_dropped() {
        let g = graph(&[(
            "app/views/a.rb",
            "class Views::A < Views::Base\n  def x\n    Integer(\"1\")\n    URI(\"x\")\n  end\nend\n",
        )]);
        assert!(g.edges.is_empty());
    }

    #[test]
    fn controllers_and_erb_become_entries() {
        let g = graph(&[
            ("app/views/wins/show.rb", "class Views::Wins::Show < Views::Base\nend\n"),
            ("app/controllers/wins_controller.rb", "class WinsController < ApplicationController\n  def show\n    render Views::Wins::Show.new(win: w)\n  end\nend\n"),
            ("app/views/wins/update.turbo_stream.erb", "<%= render Views::Wins::Show.new(win: @win) %>\n"),
        ]);
        assert_eq!(g.nodes["WinsController"].kind, Kind::Entry);
        assert_eq!(g.nodes["app/views/wins/update.turbo_stream.erb"].kind, Kind::Entry);
        assert_eq!(g.callers("Views::Wins::Show").len(), 2);
        assert_eq!(g.roots().len(), 2);
    }

    #[test]
    fn repeated_calls_are_counted_once() {
        let g = graph(&[
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/views/a.rb", "class Views::A < Views::Base\n  def x\n    Win(a: 1)\n    Win(a: 2)\n  end\nend\n"),
        ]);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].count, 2);
    }

    #[test]
    fn controllers_do_not_guess_the_components_namespace() {
        let g = graph(&[
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/controllers/x_controller.rb", "class XController\n  def x\n    Win.new(content: 1)\n  end\nend\n"),
        ]);
        assert!(g.edges.is_empty(), "Win.new in a controller is the model");
    }

    #[test]
    fn nests_block_contents_under_the_block_owner() {
        let g = graph(&[
            ("app/components/container.rb", "class Components::Container < Components::Base\nend\n"),
            ("app/components/heading.rb", "class Components::Heading < Components::Base\nend\n"),
            ("app/views/a.rb", "class Views::A < Views::Base\n  def view_template\n    Container do\n      div do\n        Heading { 1 }\n      end\n    end\n    Container do\n      Heading { 2 }\n    end\n  end\nend\n"),
        ]);
        let slots = &g.nested["Views::A"];
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].name, "Components::Container");
        assert_eq!(slots[0].count, 2);
        assert_eq!(slots[0].inside[0].name, "Components::Heading");
        assert_eq!(slots[0].inside[0].count, 2);
        let inside = g.inside_of("Components::Container");
        assert_eq!(inside[0].0, "Views::A");
        assert_eq!(inside[0].1[0].inside[0].name, "Components::Heading");
    }

    #[test]
    fn helper_methods_are_folded_in_where_they_are_called() {
        let g = graph(&[
            ("app/components/container.rb", "class Components::Container < Components::Base\nend\n"),
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/views/a.rb", "class Views::A < Views::Base\n  def view_template\n    Container do\n      render_grid\n    end\n  end\n\n  private\n\n  def render_grid\n    Win()\n  end\nend\n"),
        ]);
        let slots = &g.nested["Views::A"];
        assert_eq!(slots.len(), 1, "render_grid is not a top-level part of the page");
        assert_eq!(slots[0].inside[0].name, "Components::Win");
    }

    #[test]
    fn recursive_helper_methods_stop() {
        let g = graph(&[
            ("app/components/win.rb", "class Components::Win < Components::Base\nend\n"),
            ("app/views/a.rb", "class Views::A < Views::Base\n  def view_template\n    walk\n  end\n\n  def walk\n    Win()\n    walk\n  end\nend\n"),
        ]);
        assert_eq!(g.nested["Views::A"][0].name, "Components::Win");
    }

    #[test]
    fn unused_skips_base_classes() {
        let g = graph(&[
            ("app/components/base.rb", "class Components::Base < Phlex::HTML\nend\n"),
            ("app/components/lonely.rb", "class Components::Lonely < Components::Base\nend\n"),
        ]);
        let unused: Vec<_> = g.unused().iter().map(|n| n.name.clone()).collect();
        assert_eq!(unused, vec!["Components::Lonely"]);
    }
}
