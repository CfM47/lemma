//! Renderings of the plan: Markdown outline, LaTeX skeleton, graph diagrams.

use crate::model::{Kind, Paper, Status};
use crate::outline::{Outline, SectionKind};
use std::fmt::Write;

fn cycle_message(p: &Paper, cycle: &[usize]) -> String {
    let names: Vec<String> = cycle.iter().map(|&v| p.label(&p.nodes[v].id)).collect();
    format!(
        "The dependencies have a cycle: {} → (back to the start). Fix it to get an outline.",
        names.join(" → ")
    )
}

fn title(p: &Paper) -> &str {
    if p.paper.title.trim().is_empty() {
        "Untitled paper"
    } else {
        &p.paper.title
    }
}

/// "Lemma 3.1 (Shared), Definition 2.1 (Graph)"
fn refs(p: &Paper, o: &Outline, ids: &[String]) -> String {
    ids.iter()
        .map(|id| match p.index(id) {
            Some(v) => format!("{} ({})", o.reference(p, v), p.nodes[v].title),
            None => format!("‹{id}›"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn markdown(p: &Paper) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {}\n", title(p));
    if !p.paper.story.trim().is_empty() {
        let _ = writeln!(s, "> {}\n", p.paper.story.trim());
    }
    if !p.paper.section.is_empty() {
        let _ = writeln!(s, "TCS section: {}\n", p.paper.section);
    }
    let o = match Outline::build(p) {
        Ok(o) => o,
        Err(cycle) => {
            let _ = writeln!(s, "**{}**", cycle_message(p, &cycle));
            return s;
        }
    };
    let conclusion = o.sections.len() + 2;

    s.push_str("## 1. Introduction\n\n### 1.1 Main results\n\n");
    if o.mains.is_empty() {
        s.push_str("- (no main result yet)\n");
    }
    for &m in &o.mains {
        let n = &p.nodes[m];
        let sec = &o.sections[o.section_of[m]];
        let _ = write!(
            s,
            "- **{}** ({}), proved in Section {}.",
            o.reference(p, m),
            n.title,
            sec.number
        );
        if !n.statement.is_empty() {
            let _ = write!(s, " {}", n.statement);
        }
        s.push('\n');
    }
    s.push_str("\n### 1.2 Related work\n\n### 1.3 Organization\n\n");
    for sec in &o.sections {
        let _ = writeln!(s, "- Section {}: {}", sec.number, sec.title);
    }
    let _ = writeln!(
        s,
        "- Section {conclusion}: Concluding remarks and open problems"
    );
    if !o.appendix.is_empty() {
        s.push_str("- Appendix A: Deferred proofs\n");
    }
    s.push('\n');

    for sec in &o.sections {
        let _ = writeln!(s, "## {}. {}\n", sec.number, sec.title);
        if sec.kind != SectionKind::Preliminaries {
            s.push_str("*Intuition paragraph: the idea is …*\n\n");
        }
        for &v in &sec.items {
            let n = &p.nodes[v];
            let _ = write!(s, "- **{}** ({}) `{}`", o.reference(p, v), n.title, n.id);
            if n.main {
                s.push_str(" — main result");
            }
            if n.kind == Kind::Known {
                let cite = if n.cite.is_empty() {
                    "citation missing"
                } else {
                    &n.cite
                };
                let _ = write!(s, " — known: {cite}");
            }
            if n.appendix && n.kind.needs_proof() {
                s.push_str(" — proof in Appendix A");
            }
            if n.status == Status::Open && n.kind.expandable() {
                s.push_str(" — *open*");
            }
            s.push('\n');
            if !n.statement.is_empty() {
                let _ = writeln!(s, "  - {}", n.statement);
            }
            if !n.uses.is_empty() {
                let _ = writeln!(s, "  - Uses: {}", refs(p, &o, &n.uses));
            }
            let intro: Vec<String> = p
                .symbols
                .iter()
                .filter(|sy| sy.introduced_by == n.id)
                .map(|sy| format!("${}$ ({})", sy.notation, sy.meaning))
                .collect();
            if !intro.is_empty() {
                let _ = writeln!(s, "  - Introduces: {}", intro.join(", "));
            }
        }
        s.push('\n');
    }
    let _ = writeln!(s, "## {conclusion}. Concluding remarks and open problems\n");
    if !o.appendix.is_empty() {
        s.push_str("## Appendix A. Deferred proofs\n\n");
        for &v in &o.appendix {
            let _ = writeln!(s, "- Proof of {} ({})", o.reference(p, v), p.nodes[v].title);
        }
        s.push('\n');
    }

    if !p.symbols.is_empty() {
        s.push_str("---\n\n## Alphabet\n\n");
        let mut families: Vec<&str> = p.families.iter().map(|f| f.name.as_str()).collect();
        families.push("");
        for fam in families {
            let syms: Vec<_> = p.symbols.iter().filter(|sy| sy.family == fam).collect();
            if syms.is_empty() {
                continue;
            }
            match p.family(fam) {
                Some(f) => {
                    let _ = writeln!(
                        s,
                        "**{}** — reserved: {}\n",
                        f.name,
                        f.letters
                            .iter()
                            .map(|l| format!("${l}$"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                }
                None => s.push_str("**other**\n\n"),
            }
            for sy in syms {
                let _ = write!(s, "- ${}$ — {}", sy.notation, sy.meaning);
                if let Some(v) = p.index(&sy.introduced_by) {
                    let _ = write!(s, " ({})", o.reference(p, v));
                }
                if !sy.macro_name.is_empty() {
                    let _ = write!(s, " `{}`", sy.macro_name);
                }
                s.push('\n');
            }
            s.push('\n');
        }
    }
    s
}

fn quoted(s: &str) -> String {
    s.replace('"', "'")
}

pub fn mermaid(p: &Paper) -> String {
    let mut s = String::from("flowchart BT\n");
    for (i, n) in p.nodes.iter().enumerate() {
        let _ = writeln!(s, "  n{i}[\"{}: {}\"]", n.kind.short(), quoted(&n.title));
    }
    for (i, n) in p.nodes.iter().enumerate() {
        for u in &n.uses {
            if let Some(j) = p.index(u) {
                let _ = writeln!(s, "  n{j} --> n{i}");
            }
        }
    }
    let mains: Vec<String> = (0..p.nodes.len())
        .filter(|&i| p.nodes[i].main)
        .map(|i| format!("n{i}"))
        .collect();
    if !mains.is_empty() {
        let _ = writeln!(
            s,
            "  classDef main stroke-width:3px\n  class {} main",
            mains.join(",")
        );
    }
    s
}

pub fn dot(p: &Paper) -> String {
    let mut s = String::from("digraph lemma {\n  rankdir=BT;\n  node [shape=box];\n");
    for (i, n) in p.nodes.iter().enumerate() {
        let style = if n.main { ", penwidth=3" } else { "" };
        let _ = writeln!(
            s,
            "  n{i} [label=\"{}: {}\"{style}];",
            n.kind.short(),
            quoted(&n.title)
        );
    }
    for (i, n) in p.nodes.iter().enumerate() {
        for u in &n.uses {
            if let Some(j) = p.index(u) {
                let _ = writeln!(s, "  n{j} -> n{i};");
            }
        }
    }
    s.push_str("}\n");
    s
}

/// "Lemma~\ref{lem:x}, Definition~\ref{def:y}"
fn tex_refs(p: &Paper, ids: &[String]) -> String {
    ids.iter()
        .map(|id| match p.node(id) {
            Some(n) => format!("{}~\\ref{{{}}}", n.kind.printed(), n.id),
            None => format!("\\ref{{{id}}}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn tex_node(p: &Paper, v: usize) -> String {
    let n = &p.nodes[v];
    let mut s = String::new();
    let opt = if n.kind == Kind::Known && !n.cite.is_empty() {
        format!("{}; {}", n.title, n.cite)
    } else {
        n.title.clone()
    };
    if n.kind == Kind::Known {
        s.push_str("% Known result: cite it precisely (\"Theorem 2 of [5]\"); no proof.\n");
    }
    let _ = writeln!(
        s,
        "\\begin{{{env}}}[{opt}]\\label{{{id}}}",
        env = n.kind.env(),
        id = n.id
    );
    if n.statement.is_empty() {
        s.push_str("TODO: statement.\n");
    } else {
        let _ = writeln!(s, "{}", n.statement);
    }
    let _ = writeln!(s, "\\end{{{}}}", n.kind.env());
    if n.kind.needs_proof() {
        if n.appendix {
            s.push_str("The proof is in Appendix~\\ref{app:proofs}.\n");
        } else {
            s.push_str("\\begin{proof}\n");
            if !n.uses.is_empty() {
                let _ = writeln!(s, "% Uses {}.", tex_refs(p, &n.uses));
            }
            s.push_str("TODO.\n\\qed\n\\end{proof}\n");
        }
    } else if !n.uses.is_empty() {
        let _ = writeln!(s, "% Uses {}.", tex_refs(p, &n.uses));
    }
    s
}

/// LaTeX skeleton for the CAS template: `(path, contents)` pairs.
pub fn latex(p: &Paper) -> Result<Vec<(String, String)>, String> {
    let o = Outline::build(p).map_err(|c| cycle_message(p, &c))?;
    let mut files = vec![];
    let mut inputs = vec![];

    let mut intro = String::from("\\section{Introduction}\\label{sec:intro}\n\n");
    if !p.paper.story.trim().is_empty() {
        let _ = writeln!(
            intro,
            "% Story: {}\n",
            p.paper.story.trim().replace('\n', " ")
        );
    }
    intro.push_str("TODO: context and computational motivation.\n\n\\subsection{Main results}\\label{sec:main-results}\n\n");
    if !o.mains.is_empty() {
        intro.push_str("\\begin{itemize}\n");
        for &m in &o.mains {
            let n = &p.nodes[m];
            let sec = &o.sections[o.section_of[m]];
            let _ = writeln!(
                intro,
                "\\item {}~\\ref{{{}}} (Section~\\ref{{{}}}): {}",
                n.kind.printed(),
                n.id,
                sec.label,
                if n.statement.is_empty() {
                    "TODO"
                } else {
                    &n.statement
                }
            );
        }
        intro.push_str("\\end{itemize}\n");
    }
    intro.push_str("\n\\subsection{Related work}\\label{sec:related}\n\nTODO.\n\n\\subsection{Organization}\n\n");
    for sec in &o.sections {
        let _ = writeln!(intro, "Section~\\ref{{{}}}: {}.", sec.label, sec.title);
    }
    intro.push_str("Section~\\ref{sec:conclusion}: concluding remarks and open problems.\n");
    files.push(("sections/01-introduction.tex".to_string(), intro));

    for sec in &o.sections {
        let mut s = format!("\\section{{{}}}\\label{{{}}}\n\n", sec.title, sec.label);
        if sec.kind != SectionKind::Preliminaries {
            s.push_str("% Intuition first: \"The idea is ...\"\n\n");
        }
        for &v in &sec.items {
            s.push_str(&tex_node(p, v));
            s.push('\n');
        }
        let tail = sec.label.trim_start_matches("sec:");
        files.push((format!("sections/{:02}-{tail}.tex", sec.number), s));
    }

    let conclusion = o.sections.len() + 2;
    files.push((
        format!("sections/{conclusion:02}-conclusion.tex"),
        "\\section{Concluding remarks and open problems}\\label{sec:conclusion}\n\nTODO.\n".into(),
    ));

    if !o.appendix.is_empty() {
        let mut s = String::from(
            "\\section{Deferred proofs}\\label{app:proofs}\n\\setcounter{equation}{0}\n\n",
        );
        for &v in &o.appendix {
            let n = &p.nodes[v];
            let _ = writeln!(
                s,
                "\\paragraph{{Proof of {}~\\ref{{{}}}.}}",
                n.kind.printed(),
                n.id
            );
            if !n.uses.is_empty() {
                let _ = writeln!(s, "% Uses {}.", tex_refs(p, &n.uses));
            }
            s.push_str("TODO.\n\\qed\n\n");
        }
        files.push(("sections/appendix-a.tex".into(), s));
    }

    for (path, _) in &files {
        let stem = path.trim_end_matches(".tex");
        if path.ends_with("appendix-a.tex") {
            inputs.push("\\appendix".to_string());
        }
        inputs.push(format!("\\input{{{stem}}}"));
    }
    let mut list = String::from("%% Generated by lemma: \\input{sections/inputs} from main.tex.\n");
    for i in &inputs {
        let _ = writeln!(list, "{i}");
    }
    files.push(("sections/inputs.tex".into(), list));
    files.push(("macros.tex".into(), macros(p, &o)));
    Ok(files)
}

fn macros(p: &Paper, o: &Outline) -> String {
    let mut s = String::from(
        "%% Notation of the paper. Define every symbol once, here.\n%% Generated by lemma from its alphabet.\n%%\n",
    );
    for f in &p.families {
        let _ = writeln!(s, "%% {:<10} {}", f.name, f.letters.join(" "));
    }
    s.push('\n');
    for sy in &p.symbols {
        let mut comment = format!("{} — {}", sy.notation, sy.meaning);
        if let Some(v) = p.index(&sy.introduced_by) {
            let _ = write!(comment, " ({})", o.reference(p, v));
        }
        if sy.macro_name.is_empty() {
            let _ = writeln!(s, "%% {comment}");
        } else {
            let _ = writeln!(
                s,
                "\\newcommand{{{}}}{{{}}} % {comment}",
                sy.macro_name, sy.notation
            );
        }
    }
    s
}
