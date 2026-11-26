# lemma

A terminal app to plan a mathematical paper before writing it: the **outline**
(which statements, in which order and section) and the **alphabet** (which
letter means what, after Halmos' *How to write mathematics*).

You state your main results and lemma asks what each one depends on, until
everything is expanded, basic or cited. The plan lives in `lemma.toml`.

## Install

```sh
cargo install --path .
```

## Commands

```sh
lemma                     # full-screen editor (press ? for keys)
lemma init                # create an empty lemma.toml
lemma outline             # print the outline as Markdown
lemma check               # check dependencies and alphabet
lemma graph [--format dot|mermaid]   # dependency graph
lemma latex --out DIR     # write the LaTeX skeleton, or add what is missing
lemma import PATH         # read statements and \refs from LaTeX into the plan
```

`-f FILE` uses another plan file. See `examples/lemma.toml`.
