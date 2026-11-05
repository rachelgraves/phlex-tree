---
name: phlex-tree
description: |
  Map how Phlex views and components call each other in a Rails app with the
  phlex-tree command. Use when asked which views use a component, what a
  view or page is built from, what sits inside a component, which components
  are unused, whether a branch changed how components are used, or to spot
  gaps in component usage and architecture drift. Triggers: phlex-tree,
  component tree, component usage, who renders, who uses this component,
  unused components, component hierarchy, Phlex call graph.
---

# phlex-tree

`phlex-tree` scans a Rails app's `app/` folder and maps every Phlex view
and component, and every call between them.

## Before every use

Run this first, every time, and follow what it says:

```sh
phlex-tree --help
```

The help is the source of truth for flags, questions, examples and limits.
Do not rely on flags remembered from an earlier session: they change.

If the command is missing, it is not installed. Tell the user to run:

```sh
cargo install --path ~/Work/phlex-tree
```

## Working with it

1. Run it from the Rails app's root, or pass that folder as the path.
2. Read results with the JSON format. Show the user the tree format.
3. Name classes in full, such as `Components::Container`. A short name
   fails when two classes share it.
4. Check a surprising result in the source file before reporting it.
   The help lists what the tool cannot see.
