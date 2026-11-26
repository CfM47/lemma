# lemma

Plan a mathematical paper before writing it: the **outline** (which statements,
in which order, in which section) and the **alphabet** (which letter means what,
in the spirit of Halmos' *How to write mathematics*).

You state your main results. lemma asks what each one depends on: other
results, definitions, or known results from the literature. It keeps asking
until everything is either expanded, basic, or cited. You can stop at any time
and add new theorems or definitions later. From the dependency graph it builds
an outline that follows the usual structure of a theoretical paper (for example
*Theoretical Computer Science*):

- Preliminaries: known results, and definitions used in more than one section;
- one section per main result, and one per key lemma behind it (a lemma that
  other results reach only through it);
- every statement after everything it uses, corollaries right after their theorem;
- proofs marked "appendix" go to Appendix A.

## Install

```sh
cargo install --path .
```

## Use

```sh
lemma                 # full-screen editor on ./lemma.toml (creates it)
lemma outline         # outline as Markdown
lemma check           # cycles, unused lemmas, missing citations, alphabet clashes
lemma graph --format dot | dot -Tpdf > deps.pdf   # or --format mermaid
lemma latex --out articulo   # sections/*.tex, sections/inputs.tex and macros.tex
```

`-f FILE` picks another plan file. The plan is a plain TOML file: edit it by
hand and keep it in git next to the paper. See `examples/lemma.toml`.

### Editor

| Key | |
|---|---|
| `i` | interview: ask what each open statement depends on |
| `Enter` | interview starting at the selected statement |
| `a` / `A` | add a statement / a main result |
| `u` / `x` | add / remove a dependency |
| `e` / `d` | edit / delete |
| `m` / `p` / `b` | toggle main result / proof in appendix / basic |
| `s` | add a symbol introduced by the selected statement |
| `1`–`4` | statements, alphabet, outline, checks |
| `P` | paper title, one-paragraph story, TCS section |
| `?` | help |

In the interview, type a title and press Enter to create a new statement, or
press ↓ to pick an existing one. Enter on an empty line closes the statement and
moves to the next open one. `Ctrl+B` marks it basic, `Ctrl+K` marks it as a known
result and asks for the citation, `Ctrl+N` skips it.

Everything is saved as you go.

### Alphabet

Families reserve letters for kinds of objects (integers `n m k`, graphs `G H`,
...). A new symbol gets the first free letter of its family. `lemma check`
reports a letter with two meanings, a letter outside its family, and a
statement that mentions a symbol without depending on the definition that
introduces it.
