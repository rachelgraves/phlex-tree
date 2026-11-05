use crate::graph::{Edge, Graph, Kind, Node, Slot, Status};
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};

pub struct Options {
    pub reverse: bool,
    pub only_changes: bool,
    pub color: bool,
}

struct Printer<'a> {
    g: &'a Graph,
    opts: &'a Options,
    expanded: HashSet<String>,
    changed: HashMap<String, bool>,
    out: String,
}

pub fn render(g: &Graph, roots: &[&str], opts: &Options) -> String {
    let mut p = Printer {
        g,
        opts,
        expanded: HashSet::new(),
        changed: HashMap::new(),
        out: String::new(),
    };
    for root in roots {
        if opts.only_changes && !p.has_change(root) {
            continue;
        }
        let status = g.nodes.get(*root).and_then(|n| n.status);
        let label = p.label(root, status, 1);
        p.out.push_str(&label);
        p.out.push('\n');
        p.walk(root, "");
    }
    p.out
}

pub fn render_nested(g: &Graph, roots: &[&str], opts: &Options) -> String {
    let mut p = new_printer(g, opts);
    for root in roots {
        let label = p.label(root, None, 1);
        p.out.push_str(&label);
        p.out.push('\n');
        p.expanded.insert(root.to_string());
        let own = g.nested.get(*root).cloned().unwrap_or_default();
        p.slots(&own, "", true);
    }
    p.out
}

pub fn render_inside(g: &Graph, target: &str, opts: &Options) -> String {
    let mut p = new_printer(g, opts);
    for (caller, slots) in g.inside_of(target) {
        let label = p.label(caller, None, 1);
        p.out.push_str(&label);
        p.out.push('\n');
        p.slots(&slots, "", false);
    }
    p.out
}

fn new_printer<'a>(g: &'a Graph, opts: &'a Options) -> Printer<'a> {
    Printer {
        g,
        opts,
        expanded: HashSet::new(),
        changed: HashMap::new(),
        out: String::new(),
    }
}

impl Printer<'_> {
    fn slots(&mut self, slots: &[Slot], prefix: &str, expand: bool) {
        let last = slots.len().saturating_sub(1);
        for (i, slot) in slots.iter().enumerate() {
            let (branch, indent) = if i == last {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };
            let own = if expand {
                self.g.nested.get(&slot.name).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            let repeat = !own.is_empty() && self.expanded.contains(&slot.name);
            let mut line = format!("{prefix}{branch}{}", self.label(&slot.name, None, slot.count));
            if repeat {
                line.push_str(&self.dim(" (see above)"));
            }
            self.out.push_str(&line);
            self.out.push('\n');
            let mut kids: Vec<Slot> = Vec::new();
            if !repeat && !own.is_empty() {
                self.expanded.insert(slot.name.clone());
                kids.extend(own);
            }
            kids.extend(slot.inside.iter().cloned());
            self.slots(&kids, &format!("{prefix}{indent}"), expand);
        }
    }

    fn next(&self, name: &str) -> Vec<&Edge> {
        if self.opts.reverse {
            self.g.callers(name)
        } else {
            self.g.children(name)
        }
    }

    fn target<'e>(&self, e: &'e Edge) -> &'e str {
        if self.opts.reverse { &e.from } else { &e.to }
    }

    fn has_change(&mut self, name: &str) -> bool {
        if let Some(&c) = self.changed.get(name) {
            return c;
        }
        self.changed.insert(name.to_string(), false);
        let own = self.g.nodes.get(name).is_some_and(|n| n.status.is_some());
        let edges: Vec<(bool, String)> = self
            .next(name)
            .iter()
            .map(|e| (e.status.is_some(), self.target(e).to_string()))
            .collect();
        let result = own
            || edges
                .into_iter()
                .any(|(edge_changed, t)| edge_changed || self.has_change(&t));
        self.changed.insert(name.to_string(), result);
        result
    }

    fn walk(&mut self, name: &str, prefix: &str) {
        if !self.expanded.insert(name.to_string()) {
            return;
        }
        let mut edges: Vec<(String, Option<Status>, usize)> = self
            .next(name)
            .iter()
            .map(|e| (self.target(e).to_string(), e.status, e.count))
            .collect();
        if self.opts.only_changes {
            edges.retain(|(t, status, _)| status.is_some() || self.has_change(t));
        }
        let last = edges.len().saturating_sub(1);
        for (i, (target, status, count)) in edges.into_iter().enumerate() {
            let (branch, indent) = if i == last {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };
            let mut line = format!("{prefix}{branch}{}", self.label(&target, status, count));
            let repeat = self.expanded.contains(&target) && !self.next(&target).is_empty();
            if repeat {
                line.push_str(&self.dim(" (see above)"));
            }
            self.out.push_str(&line);
            self.out.push('\n');
            if !repeat {
                self.walk(&target, &format!("{prefix}{indent}"));
            }
        }
    }

    fn label(&self, name: &str, status: Option<Status>, count: usize) -> String {
        let node = self.g.nodes.get(name);
        let mark = match status {
            Some(Status::Added) => "+ ",
            Some(Status::Removed) => "- ",
            None => "",
        };
        let mut text = format!("{mark}{name}");
        if count > 1 {
            text.push_str(&format!(" ×{count}"));
        }
        if !self.opts.color {
            return text;
        }
        match status {
            Some(Status::Added) => text.green().bold().to_string(),
            Some(Status::Removed) => text.red().bold().to_string(),
            None => paint_kind(&text, node),
        }
    }

    fn dim(&self, s: &str) -> String {
        if self.opts.color { s.dimmed().to_string() } else { s.to_string() }
    }
}

fn paint_kind(text: &str, node: Option<&Node>) -> String {
    match node.map(|n| n.kind) {
        Some(Kind::Component) => text.cyan().to_string(),
        Some(Kind::View) => text.blue().to_string(),
        Some(Kind::Layout) => text.magenta().to_string(),
        Some(Kind::Entry) => text.yellow().to_string(),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn opts() -> Options {
        Options { reverse: false, only_changes: false, color: false }
    }

    #[test]
    fn cycles_do_not_loop_forever() {
        let scans = vec![
            scan("app/components/a.rb", "class Components::A < Components::Base\n  def x\n    B()\n  end\nend\n"),
            scan("app/components/b.rb", "class Components::B < Components::Base\n  def x\n    A()\n  end\nend\n"),
        ];
        let g = Graph::build(&scans);
        let out = render(&g, &["Components::A"], &opts());
        assert_eq!(out, "Components::A\n└── Components::B\n    └── Components::A (see above)\n");
    }

    #[test]
    fn nested_shows_block_contents_under_the_block_owner() {
        let scans = vec![
            scan("app/components/container.rb", "class Components::Container < Components::Base\nend\n"),
            scan("app/components/heading.rb", "class Components::Heading < Components::Base\nend\n"),
            scan("app/components/win.rb", "class Components::Win < Components::Base\n  def view_template\n    Heading()\n  end\nend\n"),
            scan("app/views/a.rb", "class Views::A < Views::Base\n  def view_template\n    Container do\n      Heading { 1 }\n      Win()\n    end\n  end\nend\n"),
        ];
        let g = Graph::build(&scans);
        let out = render_nested(&g, &["Views::A"], &opts());
        assert_eq!(out, "Views::A\n└── Components::Container\n    ├── Components::Heading\n    └── Components::Win\n        └── Components::Heading\n");
        let inside = render_inside(&g, "Components::Container", &opts());
        assert_eq!(inside, "Views::A\n└── Components::Container\n    ├── Components::Heading\n    └── Components::Win\n");
    }

    #[test]
    fn draws_branches() {
        let scans = vec![
            scan("app/components/a.rb", "class Components::A < Components::Base\nend\n"),
            scan("app/components/b.rb", "class Components::B < Components::Base\nend\n"),
            scan("app/views/v.rb", "class Views::V < Views::Base\n  def x\n    A()\n    B()\n    B()\n  end\nend\n"),
        ];
        let g = Graph::build(&scans);
        let out = render(&g, &["Views::V"], &opts());
        assert_eq!(out, "Views::V\n├── Components::A\n└── Components::B ×2\n");
    }
}
