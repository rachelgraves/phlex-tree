# phlex-tree

See how the [Phlex](https://www.phlex.fun) views and components in a Rails
app call each other.

It is fast: an app with 270 views and components takes about 25 milliseconds.

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

`phlex-tree --help` explains every option, the JSON fields and the limits.

## Ways to see the hierarchy

The examples below come from a small made-up bookshop app.

It has a `BookCard` component built from `Card`, `CardTitle` and `Button`.
Three pages use it: a list of books, one book, and an author page.

Each view answers a different question, so pick the one that fits.

### 1. Everything, from the top

```sh
phlex-tree
```

```text
AuthorsController
└── Views::Authors::Show
    ├── Components::Avatar
    ├── Components::BookCard
    │   ├── Components::Button
    │   ├── Components::Card
    │   └── Components::CardTitle
    └── Components::Container
BooksController
├── Views::Books::Index
│   ├── Components::BookCard (see above)
│   ├── Components::Button
│   ├── Components::Container
│   └── Components::EmptyState
│       └── Components::Card
└── Views::Books::Show
    ├── Components::Button
    ├── Components::Card
    ├── Components::CardTitle
    └── Components::Container
```

**Use it when** you are new to an app and want the whole map: which
controllers render which views, and what each view reaches.

A class shown before says `(see above)` instead of repeating its tree.

### 2. One page

```sh
phlex-tree --root Views::Books::Index
```

```text
Views::Books::Index
├── Components::BookCard
│   ├── Components::Button
│   ├── Components::Card
│   └── Components::CardTitle
├── Components::Button
├── Components::Container
└── Components::EmptyState
    └── Components::Card
```

**Use it when** you are about to change a page and want to know
everything it depends on.

### 3. One page, laid out as it renders

```sh
phlex-tree --nested --root Views::Books::Index
```

```text
Views::Books::Index
└── Components::Container
    ├── Components::BookCard
    │   └── Components::Card
    │       ├── Components::CardTitle
    │       └── Components::Button
    └── Components::EmptyState
        ├── Components::Card
        └── Components::Button
```

The plain tree in view 2 says the page calls `Button`. The nested tree
shows where: inside the empty state.

Calls inside a component's block sit under that component. So do calls
in helper methods called from that block.

**Use it when** you want to see how a page is put together, the way you
would read it in the browser.

### 4. Who uses a component

```sh
phlex-tree --callers Components::Card
```

```text
Components::Card
├── Components::BookCard
│   ├── Views::Authors::Show
│   │   └── AuthorsController
│   └── Views::Books::Index
│       └── BooksController
├── Components::EmptyState
│   └── Views::Books::Index (see above)
└── Views::Books::Show
    └── BooksController
```

This tree runs upwards, from `Card` to the controllers that reach it.

**Use it when** you are about to change a component and need every page
the change will touch.

### 5. What gets put inside a component

```sh
phlex-tree --nested --callers Components::Card
```

```text
Components::BookCard
└── Components::Card
    ├── Components::CardTitle
    └── Components::Button
Components::EmptyState
└── Components::Card
Views::Books::Show
└── Components::Card
    ├── Components::CardTitle
    └── Components::Button
```

Look at the first and last entries. `Views::Books::Show` builds the same
title and button inside a `Card` by hand, instead of using `BookCard`.

**Use it when** you want to spot gaps: places that rebuild a component
instead of using it, or use a component in an odd way.

### 6. Components nothing uses

```sh
phlex-tree --unused
```

```text
Components::Badge  app/components/badge.rb:1
```

**Use it when** you are tidying up. Check before deleting anything:
see "What it cannot see" below.

### 7. What a branch changed

On a branch, someone rebuilt the author page:

```sh
phlex-tree --diff
```

```text
AuthorsController
└── Views::Authors::Show
    ├── + Components::Card
    ├── + Components::CardTitle
    ├── + Views::Books::Show
    └── - Components::BookCard

3 new and 1 removed calls, 0 new and 0 removed classes
```

This shows two changes worth a question in review:

1. The page stopped using `BookCard` and builds its own card instead.
2. A view now renders another whole view inside itself.

It compares your working copy with the point where your branch left
`main`, the way a pull request does. Name another base with
`--diff develop`.

Only the changed parts of the tree are shown. Add `--all` to see
everything.

**Use it when** you review a pull request and want to catch changes to
how the app is put together.

### 8. For scripts, agents and browsing

```sh
phlex-tree --format json
phlex-tree --format html -o map.html
```

JSON lists every class and every call, with the file and line:

```json
{
  "from": "Views::Books::Index",
  "to": "Components::BookCard",
  "via": "shorthand",
  "file": "app/views/books/index.rb",
  "line": 4,
  "count": 1
}
```

The HTML page has a searchable tree. Pick a class to see who calls it,
what it calls, and a graph drawn with [Mermaid](https://mermaid.js.org),
loaded from a CDN. With `--diff`, it colours added calls green and
removed ones red.

**Use JSON when** a script or coding agent needs to read the results.
**Use HTML when** you want to click around a big app.

## What it finds

It reads every `.rb` and `.erb` file under `app/` and finds three kinds of
call:

| Kind      | Example                                         |
|-----------|-------------------------------------------------|
| shorthand | `BookCard(book: b)`, `Card do`, `CardTitle { }` |
| render    | `render Components::BookCard.new(...)`          |
| new       | `Components::BookCard.new(...)`                 |

Shorthand names resolve the way Ruby does, ending with the `Components`
namespace. A name only counts when a class with that name is defined in
`app/components` or `app/views`.

Controllers, ERB templates, mailers and jobs that render a Phlex class are
**entry points**. They are the tops of the tree.

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
