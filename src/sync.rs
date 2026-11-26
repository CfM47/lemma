//! Keeping the plan and the LaTeX sources in step, both ways.
//!
//! - `import`: statements, labels and `\ref`s of a manuscript become nodes and dependencies.
//! - `sync`: statements of the plan missing from the manuscript get a stub, placed in
//!   their section next to their neighbours. Nothing already written is touched.

use crate::export::{self, Role};
use crate::model::{Kind, Node, Paper, Status, valid_id};
use crate::tex::{self, TexDoc};
use anyhow::{Result, anyhow};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ImportReport {
    pub files: usize,
    pub added: Vec<String>,
    pub links: usize,
    pub refused: Vec<String>,
    pub mains: Vec<String>,
    pub unlabeled: usize,
    pub title: Option<String>,
}

/// LaTeX labels can hold characters ids do not allow.
fn id_of(label: &str) -> String {
    if valid_id(label) {
        return label.to_string();
    }
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if s.is_empty() { "x".into() } else { s }
}

/// Merges the statements of `doc` into the plan. Existing ids are kept as they are;
/// dependencies found in the sources are added to them too.
pub fn import(p: &mut Paper, doc: &TexDoc) -> ImportReport {
    let mut rep = ImportReport {
        files: doc.files.len(),
        unlabeled: doc.unlabeled,
        ..Default::default()
    };

    let mut proofs: HashMap<&str, (Vec<String>, bool)> = HashMap::new();
    for pr in &doc.proofs {
        if let Some(t) = &pr.target {
            let e = proofs.entry(t).or_default();
            e.0.extend(pr.refs.iter().cloned());
            e.1 |= pr.deferred;
        }
    }

    for s in &doc.statements {
        let id = id_of(&s.label);
        if p.node(&id).is_some() {
            continue;
        }
        let proof = proofs.get(s.label.as_str());
        let status = match s.kind {
            Kind::Known => Status::Basic,
            k if k.needs_proof() && proof.is_none() => Status::Open,
            _ => Status::Expanded,
        };
        p.nodes.push(Node {
            id,
            kind: s.kind,
            title: s.title.clone(),
            statement: s.statement.clone(),
            main: false,
            status,
            uses: vec![],
            cite: s.cite.clone(),
            appendix: s.kind.needs_proof() && proof.is_some_and(|x| x.1),
            symbols: vec![],
        });
        rep.added.push(s.label.clone());
    }

    for s in &doc.statements {
        let from = id_of(&s.label);
        let mut deps = s.refs.clone();
        if let Some((r, _)) = proofs.get(s.label.as_str()) {
            deps.extend(r.iter().cloned());
        }
        for d in deps {
            let to = id_of(&d);
            if to == from
                || p.node(&to).is_none()
                || p.node(&from).is_some_and(|n| n.uses.contains(&to))
            {
                continue;
            }
            match p.link(&from, &to) {
                Ok(()) => rep.links += 1,
                Err(e) => rep.refused.push(e),
            }
        }
    }

    if !p.nodes.iter().any(|n| n.main) {
        // A theorem that only corollaries, remarks and examples build on.
        let used: HashSet<String> = p
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, Kind::Corollary | Kind::Remark | Kind::Example))
            .flat_map(|n| n.uses.iter().cloned())
            .collect();
        for n in &mut p.nodes {
            if n.kind == Kind::Theorem && !used.contains(&n.id) {
                n.main = true;
                rep.mains.push(n.id.clone());
            }
        }
    }
    if p.paper.title.is_empty()
        && let Some(t) = &doc.title
    {
        p.paper.title = t.clone();
        rep.title = Some(t.clone());
    }
    rep
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub created: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
    pub inserted: Vec<String>,
    pub notes: Vec<String>,
}

/// Inserts `stub` at `pos`, keeping a blank line around it.
fn insert_at(text: &mut String, pos: usize, stub: &str) {
    let before = &text[..pos];
    let mut chunk = String::new();
    if pos > 0 && !before.ends_with('\n') {
        chunk.push('\n');
    }
    if pos > 0 && !before.ends_with("\n\n") {
        chunk.push('\n');
    }
    chunk.push_str(stub);
    if pos < text.len() {
        chunk.push('\n');
    }
    text.insert_str(pos, &chunk);
}

fn defines(texts: &BTreeMap<PathBuf, String>, m: &str) -> bool {
    let forms = [
        format!("\\newcommand{{{m}}}"),
        format!("\\newcommand*{{{m}}}"),
        format!("\\newcommand{m}"),
        format!("\\renewcommand{{{m}}}"),
        format!("\\providecommand{{{m}}}"),
        format!("\\DeclareMathOperator{{{m}}}"),
        format!("\\def{m}"),
    ];
    texts.values().any(|t| {
        let t = tex::blank_comments(t);
        forms.iter().any(|f| t.contains(f.as_str()))
    })
}

fn rel(dir: &Path, f: &Path) -> String {
    f.strip_prefix(dir)
        .unwrap_or(f)
        .to_string_lossy()
        .into_owned()
}

/// Writes the skeleton into `dir`. Existing files only receive what they lack;
/// with `force` every generated file is rewritten.
pub fn sync(p: &Paper, dir: &Path, force: bool) -> Result<SyncReport> {
    let o = export::cycle_error(p).map_err(|e| anyhow!(e))?;
    let generated = export::latex(p).map_err(|e| anyhow!(e))?;
    let mut rep = SyncReport::default();

    let mut original: BTreeMap<PathBuf, String> = BTreeMap::new();
    for f in tex::tex_files(dir)? {
        let text = fs::read_to_string(&f)?;
        original.insert(f, text);
    }
    let fresh = original.is_empty();
    let mut texts = original.clone();
    if force {
        for f in &generated {
            texts.insert(dir.join(&f.path), f.contents.clone());
        }
    }

    let mut label_at: HashMap<String, PathBuf> = HashMap::new();
    for (f, t) in &texts {
        for (l, _) in tex::labels(&tex::blank_comments(t)) {
            label_at.entry(l).or_insert_with(|| f.clone());
        }
    }

    // Deferred proofs go to the appendix that already exists, under its own label.
    let mut app_labels: Vec<(&String, &PathBuf)> = label_at
        .iter()
        .filter(|(l, _)| l.starts_with("app:"))
        .collect();
    app_labels.sort_by_key(|(l, f)| (*l != "app:proofs", (*f).clone(), (*l).clone()));
    let (app_label, app_file) = match app_labels.first() {
        Some((l, f)) => ((*l).clone(), Some((*f).clone())),
        None => ("app:proofs".to_string(), None),
    };

    let mut in_order: Vec<PathBuf> = vec![];
    for g in &generated {
        let default = dir.join(&g.path);
        match g.role {
            Role::Intro | Role::Conclusion => {
                let label = if g.role == Role::Intro {
                    "sec:intro"
                } else {
                    "sec:conclusion"
                };
                let target = label_at.get(label).cloned().unwrap_or(default);
                match texts.get(&target) {
                    None => {
                        texts.insert(target.clone(), g.contents.clone());
                    }
                    Some(t) if g.role == Role::Intro => {
                        let refs = tex::refs(&tex::blank_comments(t));
                        for &m in &o.mains {
                            if !refs.contains(&p.nodes[m].id) {
                                rep.notes.push(format!(
                                    "the introduction ({}) does not mention main result {}",
                                    rel(dir, &target),
                                    p.label(&p.nodes[m].id)
                                ));
                            }
                        }
                    }
                    Some(_) => {}
                }
                in_order.push(target);
            }
            Role::Section(si) => {
                let sec = &o.sections[si];
                let target = label_at.get(&sec.label).cloned().unwrap_or(default);
                if !texts.contains_key(&target) {
                    texts.insert(target.clone(), export::section_header(sec));
                    label_at.insert(sec.label.clone(), target.clone());
                }
                for (k, &v) in sec.items.iter().enumerate() {
                    let id = p.nodes[v].id.clone();
                    if let Some(f) = label_at.get(&id) {
                        if *f != target {
                            rep.notes.push(format!(
                                "{} is in {}; the outline puts it in Section {} “{}” ({})",
                                p.label(&id),
                                rel(dir, f),
                                sec.number,
                                sec.title,
                                rel(dir, &target)
                            ));
                        }
                        continue;
                    }
                    let text = texts.get_mut(&target).expect("inserted above");
                    let t = tex::blank_comments(text);
                    let labels = tex::labels(&t);
                    let start = labels
                        .iter()
                        .find(|(l, _)| *l == sec.label)
                        .map_or(0, |x| x.1);
                    let end = tex::section_end(&t, start);
                    let next = sec.items[k + 1..].iter().find_map(|&w| {
                        labels
                            .iter()
                            .find(|(l, at)| *l == p.nodes[w].id && (start..end).contains(at))
                            .map(|x| x.1)
                    });
                    let stub = export::tex_node(p, v)
                        .replace("\\ref{app:proofs}", &format!("\\ref{{{app_label}}}"));
                    match next {
                        Some(at) => insert_at(text, tex::env_line_start(&t, at), &stub),
                        None => insert_at(text, end, &stub),
                    }
                    label_at.insert(id.clone(), target.clone());
                    rep.inserted
                        .push(format!("{} → {}", p.label(&id), rel(dir, &target)));
                }
                in_order.push(target);
            }
            Role::Appendix => {
                let target = app_file.clone().unwrap_or(default);
                let text = texts
                    .entry(target.clone())
                    .or_insert_with(|| export::APPENDIX_HEADER.to_string());
                for &v in &o.appendix {
                    let id = &p.nodes[v].id;
                    if !tex::refs(&tex::blank_comments(text)).contains(id) {
                        let end = text.len();
                        insert_at(text, end, &export::appendix_proof(p, v));
                        rep.inserted.push(format!(
                            "proof of {} → {}",
                            p.label(id),
                            rel(dir, &target)
                        ));
                    }
                }
                in_order.push(target);
            }
            Role::Macros => {
                if let std::collections::btree_map::Entry::Vacant(e) = texts.entry(default.clone())
                {
                    e.insert(g.contents.clone());
                    continue;
                }
                for sy in p.symbols.iter().filter(|s| !s.macro_name.is_empty()) {
                    if !defines(&texts, &sy.macro_name) {
                        let text = texts.get_mut(&default).unwrap();
                        let end = text.len();
                        insert_at(text, end, &format!("{}\n", export::macro_line(p, &o, sy)));
                        rep.inserted.push(format!("{} → {}", sy.macro_name, g.path));
                    }
                }
            }
            Role::Inputs => {}
        }
    }

    // The input list is lemma's own file: kept up to date, created only in a fresh directory.
    let inputs = dir.join("sections/inputs.tex");
    let ours = texts
        .get(&inputs)
        .map(|t| t.starts_with(export::INPUTS_HEADER));
    if ours == Some(true) || (ours.is_none() && fresh) {
        let paths: Vec<String> = in_order.iter().map(|f| rel(dir, f)).collect();
        texts.insert(inputs.clone(), export::inputs_file(&paths));
    }

    for (path, text) in &texts {
        if original.get(path) == Some(text) {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        if original.contains_key(path) {
            rep.changed.push(path.clone());
        } else {
            rep.created.push(path.clone());
        }
    }

    // New files nobody includes yet.
    let blanked: Vec<String> = texts.values().map(|t| tex::blank_comments(t)).collect();
    for c in &rep.created {
        let stem = rel(dir, c).trim_end_matches(".tex").to_string();
        let included = blanked.iter().any(|t| {
            ["input", "include"].iter().any(|cmd| {
                t.contains(&format!("\\{cmd}{{{stem}}}"))
                    || t.contains(&format!("\\{cmd}{{{stem}.tex}}"))
            })
        });
        if !included && !fresh {
            rep.notes.push(format!(
                "{} is new: add \\input{{{stem}}} to main.tex",
                rel(dir, c)
            ));
        }
    }

    let plan: HashSet<&str> = p.nodes.iter().map(|n| n.id.as_str()).collect();
    let prefixes = Kind::ALL.map(|k| format!("{}:", k.prefix()));
    let mut strangers: Vec<&String> = label_at
        .keys()
        .filter(|l| {
            !plan.contains(l.as_str()) && prefixes.iter().any(|pre| l.starts_with(pre.as_str()))
        })
        .collect();
    strangers.sort();
    if !strangers.is_empty() {
        let names: Vec<&str> = strangers.iter().map(|s| s.as_str()).collect();
        rep.notes.push(format!(
            "in the LaTeX but not in the plan: {} (`lemma import {}` adds them)",
            names.join(", "),
            dir.display()
        ));
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Paper {
        let s = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/lemma.toml"))
            .unwrap();
        toml::from_str(&s).unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lemma-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn edges(p: &Paper) -> Vec<(String, String)> {
        let mut e: Vec<_> = p
            .nodes
            .iter()
            .flat_map(|n| n.uses.iter().map(|u| (n.id.clone(), u.clone())))
            .collect();
        e.sort();
        e
    }

    #[test]
    fn skeleton_imports_back_to_the_same_graph() {
        let p = example();
        let dir = tmp("roundtrip");
        sync(&p, &dir, false).unwrap();
        fs::write(
            dir.join("main.tex"),
            "\\documentclass{cas-sc}\n\\begin{document}\n\\input{macros}\n\\input{sections/inputs}\n\\end{document}\n",
        )
        .unwrap();
        let doc = TexDoc::read(std::slice::from_ref(&dir)).unwrap();
        let mut q = Paper::new();
        let rep = import(&mut q, &doc);
        fs::remove_dir_all(&dir).unwrap();
        assert!(rep.refused.is_empty(), "{:?}", rep.refused);
        assert_eq!(edges(&q), edges(&p));
        let known = q.node("thm:courcelle").unwrap();
        assert_eq!(known.kind, Kind::Known);
        assert_eq!(known.cite, "Theorem 1 of [Courcelle 1990]");
        assert!(q.node("lem:compression").unwrap().appendix);
        assert_eq!(rep.mains, vec!["thm:recognition"]);
        assert_eq!(
            q.node("thm:recognition").unwrap().statement,
            p.node("thm:recognition").unwrap().statement
        );
    }

    #[test]
    fn sync_inserts_without_touching_written_text() {
        let mut p = example();
        let dir = tmp("sync");
        sync(&p, &dir, false).unwrap();
        let file = dir.join("sections/03-compression.tex");
        let written = fs::read_to_string(&file)
            .unwrap()
            .replace("TODO.\n\\qed", "Real proof.\n\\qed");
        fs::write(&file, &written).unwrap();

        // A new lemma used by Compression, and a symbol with a macro.
        let id = p.add_node(Kind::Lemma, "Pumping");
        p.link("lem:compression", &id).unwrap();
        p.symbols[1].macro_name = "\\width".into();
        let rep = sync(&p, &dir, false).unwrap();

        let after = fs::read_to_string(&file).unwrap();
        assert!(after.contains("Real proof."));
        let pumping = after.find("\\label{lem:pumping}").unwrap();
        assert!(pumping < after.find("\\label{lem:compression}").unwrap());
        assert!(after.find("\\label{lem:degree}").unwrap() < pumping);
        assert!(
            fs::read_to_string(dir.join("macros.tex"))
                .unwrap()
                .contains("\\newcommand{\\width}{k}")
        );
        assert_eq!(rep.inserted.len(), 2, "{:?}", rep.inserted);
        assert!(rep.created.is_empty());

        // Nothing new: nothing changes.
        let again = sync(&p, &dir, false).unwrap();
        assert!(
            again.changed.is_empty() && again.inserted.is_empty(),
            "{again:?}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sync_reports_strangers_and_moves() {
        let p = example();
        let dir = tmp("notes");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("main.tex"),
            "\\documentclass{cas-sc}\n\\begin{document}\n\\section{Intro}\\label{sec:intro}\n\\begin{lemma}\\label{lem:degree}\nx\n\\end{lemma}\n\\begin{lemma}\\label{lem:extra}\ny\n\\end{lemma}\n\\end{document}\n",
        )
        .unwrap();
        let rep = sync(&p, &dir, false).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(
            rep.notes.iter().any(|n| n.contains("lem:extra")),
            "{:?}",
            rep.notes
        );
        assert!(
            rep.notes
                .iter()
                .any(|n| n.contains("“Degree bound” is in main.tex")),
            "{:?}",
            rep.notes
        );
        assert!(
            rep.notes
                .iter()
                .any(|n| n.contains("add \\input{sections/03-compression}")),
            "{:?}",
            rep.notes
        );
        assert!(
            rep.notes
                .iter()
                .any(|n| n.contains("does not mention main result")),
            "{:?}",
            rep.notes
        );
    }
}
