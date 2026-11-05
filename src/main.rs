use clap::{Parser, ValueEnum};
use phlex_tree::diff::{self, Summary};
use phlex_tree::graph::Graph;
use phlex_tree::render::{html, json, tree};
use phlex_tree::scan::{self, FileScan};
use rayon::prelude::*;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Tree,
    Json,
    Html,
}

#[derive(Clone, Copy, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

const ABOUT: &str = "Map which Phlex views and components call each other.";

const LONG_ABOUT: &str = "\
Map which Phlex views and components call each other.

It scans app/ for Phlex classes and finds every call between them:
  shorthand      BookCard(book: b), Card do ... end, CardTitle { ... }
  render         render Components::BookCard.new(...)
  new            Components::BookCard.new(...), such as in a turbo stream
Controllers, ERB templates, mailers and jobs that render a Phlex class
are entry points: the tops of the tree.

Three questions it answers:
  What does this class's own code call?     --root NAME
  Who uses this class?                      --callers NAME
  What gets put inside this component?      --nested --callers NAME
Add --nested to --root, or use it alone, to see calls laid out as the
page is: calls inside a component's block, and inside helper methods
called from that block, sit under that component.";

const AFTER_HELP: &str = "\
Examples:
  phlex-tree                                    every entry point and what it calls
  phlex-tree --root Views::Books::Index         one view and everything below it
  phlex-tree --nested --root Views::Books::Index the same, laid out as the page is
  phlex-tree --callers Components::Card         every class that uses Card
  phlex-tree --nested --callers Components::Card
                                                what each caller puts inside Card
  phlex-tree --unused                           components nothing calls
  phlex-tree --diff                             what this branch changed, against main
  phlex-tree --diff origin/main --format json   the same, for an agent
  phlex-tree --format html -o map.html          a page to browse, with graphs
  phlex-tree --install-skill                    install the Claude Code skill

JSON (--format json):
  nodes   every class: name, file, line, kind (view, component, layout,
          entry), parent class
  edges   every call: from, to, via, file, line of the first call, count
  roots   entry points and other classes nothing calls
  unused  components nothing calls, leaving out base classes
  With --diff: added_nodes, removed_nodes, added_edges, removed_edges,
  and a status of added or removed on changed nodes and edges.
  With --nested: slots, each with a name, a count and what is inside it.

What it cannot see:
  - classes built from a string, such as with constantize
  - calls through a variable, such as render component
  - a template a subclass gets from its parent class
  - helper methods defined in a module or a parent class (for --nested)
  - calls in a parent class's code when the subclass is the one rendered
Check a surprising result in the source file before acting on it.";

#[derive(Parser)]
#[command(version, about = ABOUT, long_about = LONG_ABOUT, after_long_help = AFTER_HELP)]
struct Cli {
    /// Root of the Rails app (the folder that holds app/).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Tree)]
    format: Format,

    /// Write to this file instead of standard output.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Show only the tree below this class. A short name like BookCard works when only one class has it.
    #[arg(long)]
    root: Option<String>,

    /// Show who calls this class, up to the entry points.
    #[arg(long, conflicts_with = "root")]
    callers: Option<String>,

    /// List components that nothing calls.
    #[arg(long)]
    unused: bool,

    /// Compare against a branch (default main), from where this branch left it.
    #[arg(long, num_args = 0..=1, default_missing_value = "main", value_name = "BASE")]
    diff: Option<String>,

    /// In diff mode, show unchanged parts of the tree too.
    #[arg(long)]
    all: bool,

    /// Show calls nested as the page is laid out: what sits inside each
    /// component's block. With --callers, shows what each caller puts
    /// inside that component.
    #[arg(long, conflicts_with_all = ["diff", "unused"])]
    nested: bool,

    /// Install the Claude Code skill for this tool into DIR/phlex-tree.
    /// DIR defaults to ~/.claude/skills. Use .claude/skills for one project.
    #[arg(long, num_args = 0..=1, value_name = "DIR", default_missing_value = "~/.claude/skills")]
    install_skill: Option<String>,

    /// When to use colour in the terminal tree.
    #[arg(long, value_enum, default_value_t = Color::Auto)]
    color: Color,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("phlex-tree: {e}");
            ExitCode::FAILURE
        }
    }
}

const SKILL: &str = include_str!("../skill/SKILL.md");

fn install_skill(dir: &str) -> Result<(), String> {
    let dir = match dir.strip_prefix("~/") {
        Some(rest) => {
            let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
            PathBuf::from(home).join(rest)
        }
        None => PathBuf::from(dir),
    };
    let target = dir.join("phlex-tree");
    std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let file = target.join("SKILL.md");
    std::fs::write(&file, SKILL).map_err(|e| format!("{}: {e}", file.display()))?;
    println!("Installed the skill at {}", file.display());
    Ok(())
}

fn run(cli: &Cli) -> Result<(), String> {
    if let Some(dir) = &cli.install_skill {
        return install_skill(dir);
    }
    let app = cli.path.join("app");
    if !app.is_dir() {
        return Err(format!("no app/ folder in {}", cli.path.display()));
    }

    let current = Graph::build(&scan_all(&read_tree(&cli.path)));
    let (graph, summary) = match &cli.diff {
        Some(base) => {
            let old = Graph::build(&scan_all(&diff::read_base(&cli.path, base)?));
            let (merged, summary) = diff::compare(&old, &current);
            (merged, Some(summary))
        }
        None => (current, None),
    };

    if cli.nested {
        return output(cli, nested(cli, &graph)?);
    }

    let text = match cli.format {
        Format::Json => json::render(&graph, summary.as_ref()),
        Format::Html => html::render(&graph, summary.as_ref(), &title(cli)),
        Format::Tree => terminal(cli, &graph, summary.as_ref())?,
    };

    output(cli, text)
}

fn output(cli: &Cli, text: String) -> Result<(), String> {
    match &cli.output {
        Some(path) => std::fs::write(path, text).map_err(|e| e.to_string()),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn nested(cli: &Cli, g: &Graph) -> Result<String, String> {
    let root = cli.root.as_deref().map(|n| find(g, n)).transpose()?;
    let inside = cli.callers.as_deref().map(|n| find(g, n)).transpose()?;
    match cli.format {
        Format::Html => Err("--nested works with the tree and json formats".into()),
        Format::Json => Ok(json::render_nested(g, root, inside)),
        Format::Tree => {
            let opts = tree::Options {
                reverse: false,
                only_changes: false,
                color: use_color(cli),
            };
            Ok(match (root, inside) {
                (_, Some(target)) => tree::render_inside(g, target, &opts),
                (Some(root), None) => tree::render_nested(g, &[root], &opts),
                (None, None) => tree::render_nested(g, &g.roots(), &opts),
            })
        }
    }
}

fn use_color(cli: &Cli) -> bool {
    match cli.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => {
            cli.output.is_none()
                && std::io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none()
        }
    }
}

fn title(cli: &Cli) -> String {
    let app = std::fs::canonicalize(&cli.path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "app".into());
    match &cli.diff {
        Some(base) => format!("Phlex calls in {app}, compared with {base}"),
        None => format!("Phlex calls in {app}"),
    }
}

fn terminal(cli: &Cli, g: &Graph, summary: Option<&Summary>) -> Result<String, String> {
    let color = use_color(cli);

    if cli.unused {
        return Ok(g
            .unused()
            .iter()
            .map(|n| format!("{}  {}:{}\n", n.name, n.file, n.line))
            .collect());
    }

    let opts = tree::Options {
        reverse: cli.callers.is_some(),
        only_changes: summary.is_some() && !cli.all,
        color,
    };
    let picked = cli.root.as_deref().or(cli.callers.as_deref());
    let mut text = match picked {
        Some(name) => tree::render(g, &[find(g, name)?], &opts),
        None => tree::render(g, &g.roots(), &opts),
    };
    if let Some(s) = summary {
        text.push_str(&format!(
            "\n{} new and {} removed calls, {} new and {} removed classes\n",
            s.added_edges.len(),
            s.removed_edges.len(),
            s.added_nodes.len(),
            s.removed_nodes.len(),
        ));
    }
    Ok(text)
}

fn find<'a>(g: &'a Graph, name: &str) -> Result<&'a str, String> {
    if let Some((k, _)) = g.nodes.get_key_value(name) {
        return Ok(k);
    }
    let suffix = format!("::{name}");
    let hits: Vec<&str> = g
        .nodes
        .keys()
        .filter(|k| k.ends_with(&suffix))
        .map(String::as_str)
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no class called {name}")),
        many => Err(format!("{name} could mean any of:\n  {}", many.join("\n  "))),
    }
}

fn read_tree(root: &Path) -> Vec<(String, String)> {
    let paths: Vec<PathBuf> = ignore::WalkBuilder::new(root.join("app"))
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .filter(|p| diff::wanted(&p.to_string_lossy()))
        .collect();
    paths
        .par_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().into_owned();
            std::fs::read_to_string(p).ok().map(|src| (rel, src))
        })
        .collect()
}

fn scan_all(files: &[(String, String)]) -> Vec<FileScan> {
    files.par_iter().map(|(p, s)| scan::scan(p, s)).collect()
}
