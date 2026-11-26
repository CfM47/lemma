# Ideas

Things that could be useful later, roughly by value.

## Outline

- **Manual overrides.** Pin a statement to a section, rename a section, merge
  two sections, mark a lemma as a technique even if the algorithm does not see
  it. Today the outline is fully automatic.
- **Introduction style.** Choose between the formal statement in 1.1 (style A
  of the guide) and an informal one (style B).
- **Proof length.** Flag proofs likely to need splitting into lemmas.

## Alphabet (Halmos)

- **Detect symbols automatically.** Read `$...$` in statements instead of
  filling "Mentions" by hand; warn about undefined letters or letters used
  before their definition.
- **Related letters.** If `X` is a set, suggest `x` for its elements;
  `f: X → Y` style pairs.
- **Odd conventions.** Warn about ε for large quantities, `n` for reals, etc.

## Editor

- Dependency tree view inside the TUI (down: what it uses; up: who uses it).
- Undo (`Ctrl+Z`).
- Search and filter in the statement list.
- Import and sync from inside the TUI.

## Author guide (TCS)

- **Examples and counterexamples per hypothesis.** Ask whether each hypothesis
  is needed and track the pending example.
- **Front matter checks.** Abstract ≤ 250 words, 3–5 highlights of ≤ 85
  characters, 1–7 keywords; generate `highlights.txt`.
- One-paragraph story check: problem, why it matters in computing, result,
  technique.

## LaTeX

- Detect sections by title when they have no `\label`.
- Insert the main results into the introduction, not only report them.
- Support `\begin{proof}[Proof of ...]` written as plain text without `\ref`.

## Later

- Optional LLM help: suggest missing dependencies, spot logical gaps, draft
  intuition paragraphs. Keep the core deterministic.
- CI with tests, release binaries for Linux and macOS.
