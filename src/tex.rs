//! Reading LaTeX sources: labels, theorem-like environments, references and proofs.
//!
//! Offsets always refer to the original text: comments are blanked, not removed.

use crate::model::Kind;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Replaces comments by spaces, keeping every offset. Comments starting with
/// `Uses ` (lemma writes them in proof stubs) are kept, so their references count.
pub fn blank_comments(src: &str) -> String {
    let mut out = Vec::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        let b = line.as_bytes();
        let cut = (0..b.len()).find(|&i| b[i] == b'%' && (i == 0 || b[i - 1] != b'\\'));
        match cut {
            None => out.extend_from_slice(b),
            Some(i) => {
                out.extend_from_slice(&b[..i]);
                if line[i + 1..].trim_start().starts_with("Uses ") {
                    out.push(b' ');
                    out.extend_from_slice(&b[i + 1..]);
                } else {
                    out.extend(
                        b[i..]
                            .iter()
                            .map(|&c| if c == b'\n' { b'\n' } else { b' ' }),
                    );
                }
            }
        }
    }
    String::from_utf8(out).expect("only ASCII bytes were replaced")
}

/// Balanced group starting at `open` (`{` or `[`), after optional whitespace.
/// Returns the inside and the offset just past the closing bracket.
fn group(t: &str, from: usize, open: u8, close: u8) -> Option<(String, usize)> {
    let b = t.as_bytes();
    let mut i = from;
    while i < b.len() && (b[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    if b.get(i) != Some(&open) {
        return None;
    }
    let start = i + 1;
    let mut depth = 0i32;
    for (j, &c) in b.iter().enumerate().skip(i) {
        if j > 0 && b[j - 1] == b'\\' {
            continue;
        }
        if c == open || (open == b'[' && c == b'{') {
            depth += 1;
        } else if c == close || (open == b'[' && c == b'}') {
            depth -= 1;
            if depth == 0 {
                return Some((t[start..j].to_string(), j + 1));
            }
        }
    }
    None
}

fn brace(t: &str, from: usize) -> Option<(String, usize)> {
    group(t, from, b'{', b'}')
}

/// `\label{x}` occurrences: (label, offset of the backslash).
pub fn labels(t: &str) -> Vec<(String, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(k) = t[i..].find("\\label") {
        let at = i + k;
        let after = at + "\\label".len();
        match brace(t, after) {
            Some((l, end)) if t.as_bytes().get(after) == Some(&b'{') => {
                out.push((l.trim().to_string(), at));
                i = end;
            }
            _ => i = after,
        }
    }
    out
}

/// Targets of `\ref`, `\cref`, `\Cref`, `\autoref`, `\eqref`... (lists split on commas).
pub fn refs(t: &str) -> Vec<String> {
    let b = t.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        let name = &t[i + 1..j];
        if j < b.len() && b[j] == b'*' {
            j += 1;
        }
        if name.ends_with("ref")
            && b.get(j) == Some(&b'{')
            && let Some((arg, end)) = brace(t, j)
        {
            out.extend(
                arg.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
            i = end;
        } else {
            i = j.max(i + 1);
        }
    }
    out
}

/// Kind of a theorem-like environment, by its usual names.
pub fn env_kind(env: &str) -> Option<Kind> {
    Some(match env {
        "theorem" | "thm" => Kind::Theorem,
        "lemma" | "lem" | "claim" | "fact" | "observation" => Kind::Lemma,
        "proposition" | "prop" => Kind::Proposition,
        "corollary" | "cor" => Kind::Corollary,
        "definition" | "defn" | "def" | "dfn" => Kind::Definition,
        "remark" | "rem" | "note" => Kind::Remark,
        "example" | "ex" | "exa" => Kind::Example,
        _ => return None,
    })
}

/// End of the environment `env` whose body starts at `from`: (body end, offset after `\end{env}`).
fn env_end(t: &str, env: &str, from: usize) -> Option<(usize, usize)> {
    let begin = format!("\\begin{{{env}}}");
    let end = format!("\\end{{{env}}}");
    let mut depth = 1;
    let mut i = from;
    loop {
        let e = t[i..].find(&end)? + i;
        match t[i..e].find(&begin) {
            Some(b) => {
                depth += 1;
                i += b + begin.len();
            }
            None => {
                depth -= 1;
                if depth == 0 {
                    return Some((e, e + end.len()));
                }
                i = e + end.len();
            }
        }
    }
}

/// Start of the line holding the last `\begin{` before `at`.
pub fn env_line_start(t: &str, at: usize) -> usize {
    let b = t[..at].rfind("\\begin{").unwrap_or(at);
    t[..b].rfind('\n').map_or(0, |n| n + 1)
}

/// Where a section that starts at `from` ends: the next sectioning or closing command.
pub fn section_end(t: &str, from: usize) -> usize {
    const STOPS: [&str; 9] = [
        "\\section{",
        "\\section*{",
        "\\appendix",
        "\\bibliographystyle",
        "\\bibliography{",
        "\\printbibliography",
        "\\printcredits",
        "\\end{document}",
        "\\begin{thebibliography}",
    ];
    let from = (from + 1).min(t.len());
    STOPS
        .iter()
        .filter_map(|s| t[from..].find(s).map(|k| from + k))
        .min()
        .map_or(t.len(), |k| {
            t[..k].rfind('\n').map_or(k, |n| n + 1).max(from)
        })
}

#[derive(Debug, Clone)]
pub struct TexStatement {
    pub kind: Kind,
    pub label: String,
    pub title: String,
    pub cite: String,
    pub statement: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TexProof {
    /// Statement the proof is for: named in its heading, or the one before it.
    pub target: Option<String>,
    pub refs: Vec<String>,
    /// Away from its statement ("Proof of Lemma 3.1" somewhere else).
    pub deferred: bool,
}

#[derive(Debug, Default)]
pub struct TexDoc {
    pub statements: Vec<TexStatement>,
    pub proofs: Vec<TexProof>,
    pub title: Option<String>,
    pub files: Vec<PathBuf>,
    /// Theorem-like environments without a label: they cannot be referenced.
    pub unlabeled: usize,
}

fn title_from_label(label: &str) -> String {
    let tail = label.rsplit(':').next().unwrap_or(label);
    let words = tail.replace(['-', '_'], " ");
    let mut c = words.trim().chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => label.to_string(),
    }
}

/// Splits an optional argument into title and citation.
/// `[Courcelle \cite{c90}]` and lemma's own `[Courcelle; Theorem 1 of [3]]` are citations.
fn title_and_cite(opt: &str, has_proof: bool) -> (String, String) {
    let opt = opt.trim();
    let opt = match opt.strip_prefix('{').and_then(|o| o.strip_suffix('}')) {
        Some(inner) => inner,
        None => opt,
    };
    if let Some(k) = opt.find("\\cite") {
        let title = opt[..k]
            .trim()
            .trim_end_matches([',', ';', ':', '('])
            .trim();
        return (title.to_string(), opt[k..].trim().to_string());
    }
    if !has_proof && let Some((t, c)) = opt.split_once("; ") {
        return (t.trim().to_string(), c.trim().to_string());
    }
    (opt.trim().to_string(), String::new())
}

fn clean_statement(body: &str) -> String {
    let mut s: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('%'))
        .collect::<Vec<_>>()
        .join("\n");
    for (l, _) in labels(body) {
        s = s.replace(&format!("\\label{{{l}}}"), "");
    }
    let s = s.trim();
    if s.starts_with("TODO") {
        String::new()
    } else {
        s.to_string()
    }
}

impl TexDoc {
    /// Reads `paths` (files or directories). A directory is read from its main
    /// document (`main.tex`, or files with `\documentclass`), following `\input`.
    pub fn read(paths: &[PathBuf]) -> std::io::Result<TexDoc> {
        let mut doc = TexDoc::default();
        let mut seen = HashSet::new();
        for path in paths {
            let roots = if path.is_dir() {
                roots(path)?
            } else {
                vec![path.clone()]
            };
            for r in roots {
                let base = r.parent().map(Path::to_path_buf).unwrap_or_default();
                doc.scan_file(&r, &base, &mut seen)?;
            }
        }
        Ok(doc)
    }

    fn scan_file(
        &mut self,
        file: &Path,
        base: &Path,
        seen: &mut HashSet<PathBuf>,
    ) -> std::io::Result<()> {
        let key = file.canonicalize().unwrap_or(file.to_path_buf());
        if !seen.insert(key) {
            return Ok(());
        }
        let raw = fs::read_to_string(file)?;
        self.files.push(file.to_path_buf());
        let t = blank_comments(&raw);
        if self.title.is_none()
            && let Some(k) = t.find("\\title")
        {
            let mut at = k + "\\title".len();
            if let Some((_, end)) = group(&t, at, b'[', b']') {
                at = end;
            }
            if let Some((title, _)) = brace(&t, at) {
                let title = title.split('\\').next().unwrap_or("").trim().to_string();
                if !title.is_empty() {
                    self.title = Some(title);
                }
            }
        }

        let mut last: Option<String> = None;
        let mut i = 0;
        const TOKENS: [&str; 4] = ["\\begin{", "\\paragraph", "\\input{", "\\include{"];
        while let Some((at, tok)) = TOKENS
            .iter()
            .filter_map(|tok| t[i..].find(tok).map(|k| (i + k, *tok)))
            .min_by_key(|(k, _)| *k)
        {
            match tok {
                "\\input{" | "\\include{" => {
                    let Some((name, end)) = brace(&t, at + tok.len() - 1) else {
                        break;
                    };
                    i = end;
                    let mut p = base.join(name.trim());
                    if p.extension().is_none() {
                        p.set_extension("tex");
                    }
                    if p.exists() {
                        self.scan_file(&p, base, seen)?;
                    }
                }
                "\\paragraph" => {
                    let mut after = at + tok.len();
                    if t.as_bytes().get(after) == Some(&b'*') {
                        after += 1;
                    }
                    let Some((head, end)) = brace(&t, after) else {
                        i = after;
                        continue;
                    };
                    i = end;
                    if !head.trim_start().starts_with("Proof") {
                        continue;
                    }
                    let stop = ["\\paragraph", "\\section", "\\subsection"]
                        .iter()
                        .filter_map(|s| t[end..].find(s).map(|k| end + k))
                        .min()
                        .unwrap_or(t.len())
                        .min(section_end(&t, end));
                    self.proofs.push(TexProof {
                        target: refs(&head).into_iter().next(),
                        refs: refs(&t[end..stop]),
                        deferred: true,
                    });
                    i = stop;
                }
                _ => {
                    let Some((env, after)) = brace(&t, at + "\\begin".len()) else {
                        break;
                    };
                    if env == "proof" {
                        i = self.proof(&t, after, &last);
                    } else if let Some(kind) = env_kind(&env) {
                        let (opt, body_start) = match group(&t, after, b'[', b']') {
                            Some((o, e)) => (o, e),
                            None => (String::new(), after),
                        };
                        let Some((body_end, end)) = env_end(&t, &env, body_start) else {
                            break;
                        };
                        i = end;
                        let body = &t[body_start..body_end];
                        let Some((label, _)) = labels(body).into_iter().next() else {
                            self.unlabeled += 1;
                            continue;
                        };
                        let has_proof = t[end..].trim_start().starts_with("\\begin{proof}");
                        let (mut title, cite) = title_and_cite(&opt, has_proof);
                        if title.is_empty() {
                            title = title_from_label(&label);
                        }
                        self.statements.push(TexStatement {
                            kind: if cite.is_empty() { kind } else { Kind::Known },
                            label: label.clone(),
                            title,
                            cite,
                            statement: clean_statement(&raw[body_start..body_end]),
                            refs: refs(body),
                        });
                        last = Some(label);
                    } else {
                        i = after;
                    }
                }
            }
        }
        Ok(())
    }

    /// A proof environment whose body starts at `after`; returns where scanning continues.
    fn proof(&mut self, t: &str, after: usize, last: &Option<String>) -> usize {
        let (head, body_start) = match group(t, after, b'[', b']') {
            Some((o, e)) => (o, e),
            None => (String::new(), after),
        };
        let Some((body_end, end)) = env_end(t, "proof", body_start) else {
            return t.len();
        };
        let named = refs(&head).into_iter().next();
        self.proofs.push(TexProof {
            deferred: named.is_some(),
            target: named.or_else(|| last.clone()),
            refs: refs(&t[body_start..body_end]),
        });
        end
    }
}

/// Entry documents of a directory: `main.tex`, else files with `\documentclass`, else every .tex.
fn roots(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let main = dir.join("main.tex");
    if main.exists() {
        return Ok(vec![main]);
    }
    let files = tex_files(dir)?;
    let docs: Vec<PathBuf> = files
        .iter()
        .filter(|f| {
            fs::read_to_string(f).is_ok_and(|s| blank_comments(&s).contains("\\documentclass"))
        })
        .cloned()
        .collect();
    Ok(if docs.is_empty() { files } else { docs })
}

/// Every .tex file under `dir`, sorted, skipping hidden directories.
pub fn tex_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d)? {
            let path = entry?.path();
            let hidden = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if path.is_dir() && !hidden {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "tex") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_keep_offsets() {
        let s = "a % \\label{x}\nb \\% c % Uses Lemma~\\ref{y}.\n";
        let t = blank_comments(s);
        assert_eq!(t.len(), s.len());
        assert!(labels(&t).is_empty());
        assert_eq!(refs(&t), vec!["y"]);
    }

    #[test]
    fn refs_and_labels() {
        let t = r"see \cref{lem:a, lem:b} and Theorem~\ref{thm:c}, \eqref{eq:1}. \label{x}";
        assert_eq!(refs(t), vec!["lem:a", "lem:b", "thm:c", "eq:1"]);
        assert_eq!(
            labels(t),
            vec![("x".to_string(), t.find("\\label").unwrap())]
        );
    }

    #[test]
    fn section_end_stops_at_next_section() {
        let t = "\\section{A}\\label{sec:a}\ntext\n\\section{B}\nmore";
        assert_eq!(&t[section_end(t, 0)..], "\\section{B}\nmore");
    }

    #[test]
    fn reads_statements_and_proofs() {
        let dir = std::env::temp_dir().join(format!("lemma-tex-{}", std::process::id()));
        fs::create_dir_all(dir.join("sections")).unwrap();
        fs::write(
            dir.join("main.tex"),
            "\\documentclass{cas-sc}\n\\title[mode=title]{Fast width}\n\\begin{document}\n\\input{sections/body}\n\\appendix\n\\input{sections/app}\n\\end{document}\n",
        )
        .unwrap();
        fs::write(
            dir.join("sections/body.tex"),
            r"\section{Prelim}
\begin{definition}[Graph]\label{def:graph}
A graph $G$.
\end{definition}
\begin{theorem}[Courcelle \cite{c90}]\label{thm:courcelle}
MSO is linear.
\end{theorem}
% \begin{lemma}\label{lem:old} commented out \end{lemma}
\begin{lemma}\label{lem:degree-bound}
Degrees of $G$ (Definition~\ref{def:graph}) are small.
\end{lemma}
\begin{proof}
\begin{align} x \end{align}
By Theorem~\ref{thm:courcelle}.
\end{proof}
\begin{theorem}[Main]\label{thm:main}
Linear.
\end{theorem}
The proof is in the appendix.
\begin{lemma}
No label here.
\end{lemma}
",
        )
        .unwrap();
        fs::write(
            dir.join("sections/app.tex"),
            "\\section{Proofs}\n\\paragraph{Proof of Theorem~\\ref{thm:main}.}\nUse Lemma~\\ref{lem:degree-bound}.\n",
        )
        .unwrap();
        let doc = TexDoc::read(std::slice::from_ref(&dir)).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(doc.title.as_deref(), Some("Fast width"));
        let labels: Vec<&str> = doc.statements.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["def:graph", "thm:courcelle", "lem:degree-bound", "thm:main"]
        );
        assert_eq!(doc.unlabeled, 1);
        let k = &doc.statements[1];
        assert_eq!(
            (k.kind, k.title.as_str(), k.cite.as_str()),
            (Kind::Known, "Courcelle", "\\cite{c90}")
        );
        assert_eq!(doc.statements[2].title, "Degree bound");
        assert_eq!(doc.statements[2].refs, vec!["def:graph"]);
        assert_eq!(doc.proofs.len(), 2);
        assert_eq!(doc.proofs[0].target.as_deref(), Some("lem:degree-bound"));
        assert_eq!(doc.proofs[0].refs, vec!["thm:courcelle"]);
        assert!(!doc.proofs[0].deferred);
        assert_eq!(doc.proofs[1].target.as_deref(), Some("thm:main"));
        assert!(doc.proofs[1].deferred);
    }
}
