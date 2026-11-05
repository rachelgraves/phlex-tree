# phlex-tree

See how the [Phlex](https://www.phlex.fun) views and components in a Rails
app call each other.

It is fast: a 270-class app takes about 25 milliseconds.

## What it answers

- **What does this view or component call?**
- **Who uses this component?**
- **What gets put inside this component?** For example, what every view
  puts inside a `Container`.
- **Which components does nothing use?**
- **What did this branch change?** New and removed calls, compared with
  `main`.

It is built for spotting gaps in component usage and changes to how an
app is put together, by people and by coding agents.

## Install

You need [Rust](https://rustup.rs).

```sh
cargo install --git https://github.com/rachelgraves/phlex-tree
```

## Use

Run it from the root of a Rails app:

```sh
phlex-tree
```

Some examples:

```sh
phlex-tree --root Views::Wins::Index          # one view and what it calls
phlex-tree --nested --root Views::Wins::Index # the same, laid out as the page is
phlex-tree --callers Components::Button       # who uses Button
phlex-tree --nested --callers Components::Card
                                              # what each caller puts inside Card
phlex-tree --unused                           # components nothing calls
phlex-tree --diff                             # what this branch changed
phlex-tree --format json                      # for scripts and agents
phlex-tree --format html -o map.html          # a page to browse
```

`phlex-tree --help` explains every option, the JSON fields and the limits.

## Example output

```text
Views::Wins::Index
├── Components::BackButton
└── Components::Container
    ├── Components::Heading
    ├── Components::EmptyState
    │   └── Components::EmptyStateBody
    └── Components::Win
        ├── Components::Record
        └── Components::ShareButton
```

In diff mode, new calls start with `+` and removed ones with `-`.

## What it finds

It reads every `.rb` and `.erb` file under `app/` and finds three kinds of
call:

| Kind      | Example                                      |
|-----------|----------------------------------------------|
| shorthand | `Win(win: w)`, `FormField do`, `Heading { }` |
| render    | `render Components::Win.new(...)`            |
| new       | `Components::Prompt.new(...)`                |

Shorthand names resolve the way Ruby does, ending with the `Components`
namespace. A name only counts when a class with that name is defined in
`app/components` or `app/views`.

Controllers, ERB templates, mailers and jobs that render a Phlex class are
**entry points**. They are the tops of the tree.

## Formats

1. **tree**: a coloured tree for the terminal. This is the default.
2. **json**: nodes, edges, roots and unused components, for scripts and
   agents.
3. **html**: one page with a searchable tree and a graph of each class.
   The graphs are drawn with [Mermaid](https://mermaid.js.org), loaded
   from a CDN.

## Diff mode

```sh
phlex-tree --diff            # against main
phlex-tree --diff develop    # against another branch
```

It compares your working copy with the point where your branch left the
base branch, the way a pull request does. It reads the old files straight
from git, so nothing is checked out.

## What it cannot see

It reads the code as text. It does not run it. So it misses:

- classes built from a string, such as with `constantize`
- calls through a variable, such as `render component`
- a template a subclass gets from its parent class
- helper methods defined in a module or a parent class, for `--nested`

Check a surprising result in the source before acting on it.

## Claude Code skill

The tool ships with a skill for [Claude Code](https://claude.com/claude-code).
It tells the agent to read `phlex-tree --help` before each use.

```sh
phlex-tree --install-skill                   # for every project
phlex-tree --install-skill .claude/skills    # for this project only
```

## Development

```sh
cargo test
cargo clippy
```

## Licence

MIT. See [LICENSE](LICENSE).
