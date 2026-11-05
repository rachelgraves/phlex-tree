use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Shorthand,
    Render,
    New,
    Method,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub parent: Option<String>,
    pub scope: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub scope: Vec<String>,
    pub caller: Option<String>,
    pub via: Via,
    pub line: usize,
    pub parent: Option<usize>,
    pub method: Option<String>,
}

#[derive(Debug, Default)]
pub struct FileScan {
    pub path: String,
    pub classes: Vec<ClassDef>,
    pub calls: Vec<Call>,
}

const CONST: &str = r"(?:::)?[A-Z]\w*(?:::[A-Z]\w*)*";

static DEF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(\s*)(class|module)\s+({CONST})(?:\s*<\s*({CONST}))?"
    ))
    .unwrap()
});
static END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(?:<%-?\s*)?end\b").unwrap());
static DEF_METHOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*)def\s+(?:self\.)?([a-z_]\w*[?!]?)(\s*\([^)]*\))?(\s*=[^=])?").unwrap()
});
static IDENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^\w.:@$])([a-z_]\w*[?!]?)").unwrap());
static STRING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'"#).unwrap());
static CLOSE_BRACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)\}").unwrap());
static OPENS_DO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bdo(?:\s*\|[^|]*\|)?\s*(?:-?%>)?\s*$").unwrap()
});
static RENDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"\brender[\s(]+({CONST})\.new\b")).unwrap()
});
static NEW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"({CONST})\.new\b")).unwrap());
static SHORTHAND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^\w:.@$])((?:[A-Z]\w*::)*[A-Z]\w*)(?:\(|\s+do\b|\s*\{)")
        .unwrap()
});

#[derive(PartialEq)]
enum BlockKind {
    Do,
    Brace,
}

struct Block {
    call: usize,
    indent: usize,
    kind: BlockKind,
}

struct DefFrame {
    name: String,
    indent: usize,
}

fn paren_delta(line: &str) -> i64 {
    let bare = STRING_RE.replace_all(line, "\"\"");
    bare.chars().fold(0, |d, c| match c {
        '(' => d + 1,
        ')' => d - 1,
        _ => d,
    })
}

struct Frame {
    parts: Vec<String>,
    indent: usize,
    is_class: bool,
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with("<%#")
}

fn ends_with_render(line: &str) -> bool {
    let t = line.trim_end();
    t.ends_with("render(") || t.ends_with("render")
}

pub fn scan(path: &str, src: &str) -> FileScan {
    let mut out = FileScan {
        path: path.to_string(),
        ..Default::default()
    };
    let mut stack: Vec<Frame> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut def_stack: Vec<DefFrame> = Vec::new();
    let mut defs: std::collections::HashMap<String, std::collections::HashSet<String>> =
        Default::default();
    let mut paren_depth: i64 = 0;
    let mut pending: Option<(usize, usize)> = None;
    let mut prev_line = "";

    for (idx, line) in src.lines().enumerate() {
        let lineno = idx + 1;
        if is_comment(line) {
            continue;
        }

        if let Some(c) = DEF_RE.captures(line) {
            let indent = c[1].len();
            let is_class = &c[2] == "class";
            let written = c[3].trim_start_matches("::");
            let scope = scope_of(&stack);
            let mut parts = if c[3].starts_with("::") {
                Vec::new()
            } else {
                scope.clone()
            };
            parts.extend(written.split("::").map(String::from));
            if is_class {
                out.classes.push(ClassDef {
                    name: parts.join("::"),
                    parent: c.get(4).map(|m| m.as_str().to_string()),
                    scope,
                    line: lineno,
                });
            }
            let one_liner = line.trim_end().ends_with("end");
            if !one_liner {
                stack.push(Frame {
                    parts,
                    indent,
                    is_class,
                });
            }
            prev_line = line;
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        if let Some(c) = DEF_METHOD_RE.captures(line) {
            let name = c[2].to_string();
            if let Some(class) = current_class(&stack) {
                defs.entry(class).or_default().insert(name.clone());
            }
            if c.get(4).is_none() && !line.trim_end().ends_with("end") {
                def_stack.push(DefFrame { name, indent: c[1].len() });
                paren_depth = 0;
                pending = None;
                prev_line = line;
                continue;
            }
        }
        if let Some(c) = END_RE.captures(line) {
            let indent = c[1].len();
            if blocks.last().is_some_and(|b| b.indent == indent && b.kind == BlockKind::Do) {
                blocks.pop();
            } else if def_stack.last().is_some_and(|d| d.indent == indent) {
                def_stack.pop();
            } else if stack.last().is_some_and(|f| f.indent == indent) {
                stack.pop();
            }
            prev_line = line;
            continue;
        }
        if let Some(c) = CLOSE_BRACE_RE.captures(line) {
            let indent = c[1].len();
            if blocks.last().is_some_and(|b| b.indent == indent && b.kind == BlockKind::Brace) {
                blocks.pop();
            }
        }

        let scope = scope_of(&stack);
        let caller = current_class(&stack);
        let method = def_stack.last().map(|d| d.name.clone());

        let mut found: Vec<(usize, usize, String, Via)> = Vec::new();
        let overlaps = |found: &[(usize, usize, String, Via)], s: usize, e: usize| {
            found.iter().any(|f| s < f.1 && f.0 < e)
        };
        for c in RENDER_RE.captures_iter(line) {
            let m = c.get(1).unwrap();
            found.push((m.start(), m.end(), m.as_str().into(), Via::Render));
        }
        for c in NEW_RE.captures_iter(line) {
            let m = c.get(1).unwrap();
            if overlaps(&found, m.start(), m.end()) {
                continue;
            }
            let first_on_line = line[..m.start()].trim().is_empty();
            let via = if first_on_line && ends_with_render(prev_line) {
                Via::Render
            } else {
                Via::New
            };
            found.push((m.start(), m.end(), m.as_str().into(), via));
        }
        for c in SHORTHAND_RE.captures_iter(line) {
            let m = c.get(1).unwrap();
            if overlaps(&found, m.start(), m.end()) {
                continue;
            }
            let whole_end = c.get(0).unwrap().end();
            found.push((m.start(), whole_end, m.as_str().into(), Via::Shorthand));
        }
        if method.is_some() {
            for c in IDENT_RE.captures_iter(line) {
                let m = c.get(1).unwrap();
                let rest = &line[m.end()..];
                let is_key = rest.starts_with(':') && !rest.starts_with("::");
                if !is_key && !overlaps(&found, m.start(), m.end()) {
                    found.push((m.start(), m.end(), m.as_str().into(), Via::Method));
                }
            }
        }
        found.sort_by_key(|f| f.0);

        let outer = blocks.last().map(|b| b.call);
        let mut spans: Vec<(usize, usize, usize)> = Vec::new();
        let mut open_brace: Option<usize> = None;
        let first_index = out.calls.len();
        for (start, end, name, via) in found {
            let parent = spans
                .iter()
                .filter(|s| s.0 <= start && start < s.1)
                .min_by_key(|s| s.1 - s.0)
                .map(|s| s.2)
                .or(outer);
            let idx = out.calls.len();
            out.calls.push(Call {
                name,
                scope: scope.clone(),
                caller: caller.clone(),
                via,
                line: lineno,
                parent,
                method: method.clone(),
            });
            if via == Via::Shorthand && line[..end].ends_with('{') {
                match matching_brace(line, end) {
                    Some(close) => spans.push((end, close, idx)),
                    None => open_brace = Some(idx),
                }
            }
        }
        let owner = (first_index..out.calls.len()).find(|&i| out.calls[i].via != Via::Method);
        let depth_before = paren_depth;
        paren_depth = (paren_depth + paren_delta(line)).max(0);
        if depth_before == 0 && paren_depth > 0 {
            pending = owner.map(|i| (i, indent));
        } else if depth_before > 0 && paren_depth == 0 {
            if let Some((i, at)) = pending.take() {
                if OPENS_DO_RE.is_match(line) {
                    blocks.push(Block { call: i, indent: at, kind: BlockKind::Do });
                } else if line.trim_end().ends_with('{') {
                    blocks.push(Block { call: i, indent: at, kind: BlockKind::Brace });
                }
            }
        } else if paren_depth == 0 {
            if let Some(idx) = open_brace {
                blocks.push(Block { call: idx, indent, kind: BlockKind::Brace });
            } else if let Some(i) = owner.filter(|_| OPENS_DO_RE.is_match(line)) {
                blocks.push(Block { call: i, indent, kind: BlockKind::Do });
            }
        }

        if !line.trim().is_empty() {
            prev_line = line;
        }
    }

    keep_known_methods(&mut out, &defs);
    out
}

fn current_class(stack: &[Frame]) -> Option<String> {
    stack.iter().rev().find(|f| f.is_class).map(|f| f.parts.join("::"))
}

fn keep_known_methods(
    out: &mut FileScan,
    defs: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) {
    let keep: Vec<bool> = out
        .calls
        .iter()
        .map(|c| {
            c.via != Via::Method
                || c.caller
                    .as_ref()
                    .and_then(|k| defs.get(k))
                    .is_some_and(|d| d.contains(&c.name) && c.method.as_deref() != Some(&c.name))
        })
        .collect();
    let mut new_index = vec![None; keep.len()];
    let mut next = 0;
    for (i, &k) in keep.iter().enumerate() {
        if k {
            new_index[i] = Some(next);
            next += 1;
        }
    }
    let calls = std::mem::take(&mut out.calls);
    out.calls = calls
        .into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(mut c, _)| {
            c.parent = c.parent.and_then(|p| new_index[p]);
            c
        })
        .collect();
}

fn matching_brace(line: &str, from: usize) -> Option<usize> {
    let mut depth = 1;
    for (i, ch) in line[from..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(from + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn scope_of(stack: &[Frame]) -> Vec<String> {
    stack.last().map(|f| f.parts.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &FileScan) -> Vec<(&str, Via)> {
        s.calls.iter().map(|c| (c.name.as_str(), c.via)).collect()
    }

    #[test]
    fn finds_compact_class_and_parent() {
        let s = scan("a.rb", "class Views::Wins::Show < Views::Base\nend\n");
        assert_eq!(s.classes[0].name, "Views::Wins::Show");
        assert_eq!(s.classes[0].parent.as_deref(), Some("Views::Base"));
    }

    #[test]
    fn finds_nested_module_class() {
        let src = "module Components\n  module Forms\n    class Field < Base\n      def view_template\n        Label(for: 1)\n      end\n    end\n  end\nend\n";
        let s = scan("a.rb", src);
        assert_eq!(s.classes[0].name, "Components::Forms::Field");
        assert_eq!(s.calls[0].caller.as_deref(), Some("Components::Forms::Field"));
        assert_eq!(s.calls[0].scope, vec!["Components", "Forms", "Field"]);
    }

    #[test]
    fn shorthand_with_parens_block_and_namespace() {
        let src = "class Views::A < Views::Base\n  def view_template\n    HelpButton(id: 1)\n    FormField do\n    EmptyStateTitle { t('x') }\n    Mailers::Container(x: 1)\n  end\nend\n";
        let s = scan("a.rb", src);
        assert_eq!(
            names(&s),
            vec![
                ("HelpButton", Via::Shorthand),
                ("FormField", Via::Shorthand),
                ("EmptyStateTitle", Via::Shorthand),
                ("Mailers::Container", Via::Shorthand),
            ]
        );
    }

    #[test]
    fn render_on_one_line_and_split() {
        let src = "render Components::Win.new(win: w)\nrender(\n  Views::Wins::New.new(\n)\nturbo_stream.replace(x, Components::Prompt.new)\n";
        let s = scan("c.rb", src);
        assert_eq!(
            names(&s),
            vec![
                ("Components::Win", Via::Render),
                ("Views::Wins::New", Via::Render),
                ("Components::Prompt", Via::New),
            ]
        );
    }

    #[test]
    fn erb_render() {
        let s = scan("a.html.erb", "<%= render Components::RecordGlyph.new(record: item) %>\n<%# render Components::Nope.new %>\n");
        assert_eq!(names(&s), vec![("Components::RecordGlyph", Via::Render)]);
    }

    #[test]
    fn ignores_comment_lines() {
        let s = scan("a.rb", "  # Button(x)\n  Button(x)\n");
        assert_eq!(s.calls.len(), 1);
        assert_eq!(s.calls[0].via, Via::Shorthand);
        assert_eq!(s.calls[0].line, 2);
    }

    fn parent_of<'a>(s: &'a FileScan, name: &str) -> Option<&'a str> {
        let c = s.calls.iter().find(|c| c.name == name).unwrap();
        c.parent.map(|p| s.calls[p].name.as_str())
    }

    #[test]
    fn calls_inside_do_blocks_have_the_block_owner_as_parent() {
        let src = "class Views::A < Views::Base\n  def view_template\n    Container(class: \"x\") do\n      Heading { \"Wins\" }\n      div do\n        Win(win: w)\n      end\n    end\n    Footer()\n  end\nend\n";
        let s = scan("a.rb", src);
        assert_eq!(parent_of(&s, "Container"), None);
        assert_eq!(parent_of(&s, "Heading"), Some("Container"));
        assert_eq!(parent_of(&s, "Win"), Some("Container"));
        assert_eq!(parent_of(&s, "Footer"), None);
    }

    #[test]
    fn calls_inside_one_line_braces() {
        let s = scan("a.rb", "EmptyStateImage { Munch(size: :xl) }\nOther()\n");
        assert_eq!(parent_of(&s, "Munch"), Some("EmptyStateImage"));
        assert_eq!(parent_of(&s, "Other"), None);
    }

    #[test]
    fn calls_inside_multi_line_braces() {
        let s = scan("a.rb", "    Card {\n      Title()\n    }\n    After()\n");
        assert_eq!(parent_of(&s, "Title"), Some("Card"));
        assert_eq!(parent_of(&s, "After"), None);
    }

    #[test]
    fn erb_render_blocks() {
        let src = "<%= render Components::Dialog.new(x: 1) do %>\n  <%= render Components::Form.new %>\n<% end %>\n<%= render Components::After.new %>\n";
        let s = scan("a.html.erb", src);
        assert_eq!(parent_of(&s, "Components::Form"), Some("Components::Dialog"));
        assert_eq!(parent_of(&s, "Components::After"), None);
    }

    #[test]
    fn block_end_does_not_close_the_class() {
        let src = "class Views::A < Views::Base\n  def x\n  Box do\n  end\n    Win()\n  end\nend\n";
        let s = scan("a.rb", src);
        assert_eq!(s.calls[1].caller.as_deref(), Some("Views::A"));
    }

    #[test]
    fn multi_line_call_then_do_on_a_later_line() {
        let src = "class Views::A < Views::Base\n  def x\n    EmptyState(\n      id: \"a\",\n      title: t(\"b\")\n    ) do\n      EmptyStateBody()\n    end\n    After()\n  end\nend\n";
        let s = scan("a.rb", src);
        assert_eq!(parent_of(&s, "EmptyStateBody"), Some("EmptyState"));
        assert_eq!(parent_of(&s, "After"), None);
    }

    #[test]
    fn records_calls_to_own_helper_methods() {
        let src = "class Views::A < Views::Base\n  def view_template\n    Container do\n      render_grid if ok?\n      div(class: \"x\")\n    end\n  end\n\n  private\n\n  def render_grid\n    Win()\n  end\n\n  def ok? = true\nend\n";
        let s = scan("a.rb", src);
        let methods: Vec<_> = s.calls.iter().filter(|c| c.via == Via::Method).map(|c| c.name.as_str()).collect();
        assert_eq!(methods, vec!["render_grid", "ok?"]);
        assert_eq!(parent_of(&s, "render_grid"), Some("Container"));
        let win = s.calls.iter().find(|c| c.name == "Win").unwrap();
        assert_eq!(win.method.as_deref(), Some("render_grid"));
    }

    #[test]
    fn does_not_double_count_new_as_shorthand() {
        let s = scan("a.rb", "Components::Win.new(win: w)\n");
        assert_eq!(names(&s), vec![("Components::Win", Via::New)]);
    }
}
