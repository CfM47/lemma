//! The paper as data: statements, the dependencies between them, and the alphabet.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Theorem,
    Proposition,
    Lemma,
    Corollary,
    Definition,
    /// A result from the literature: stated with a citation, never proved.
    Known,
    Remark,
    Example,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Kind::Theorem,
        Kind::Proposition,
        Kind::Lemma,
        Kind::Corollary,
        Kind::Definition,
        Kind::Known,
        Kind::Remark,
        Kind::Example,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Theorem => "Theorem",
            Kind::Proposition => "Proposition",
            Kind::Lemma => "Lemma",
            Kind::Corollary => "Corollary",
            Kind::Definition => "Definition",
            Kind::Known => "Known result",
            Kind::Remark => "Remark",
            Kind::Example => "Example",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Kind::Theorem => "Thm",
            Kind::Proposition => "Prop",
            Kind::Lemma => "Lem",
            Kind::Corollary => "Cor",
            Kind::Definition => "Def",
            Kind::Known => "Known",
            Kind::Remark => "Rem",
            Kind::Example => "Ex",
        }
    }

    /// Prefix of the LaTeX label, following the paper's guide (`thm:`, `lem:`, `def:`...).
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Theorem | Kind::Known => "thm",
            Kind::Proposition => "prop",
            Kind::Lemma => "lem",
            Kind::Corollary => "cor",
            Kind::Definition => "def",
            Kind::Remark => "rem",
            Kind::Example => "ex",
        }
    }

    /// LaTeX environment. Known results are stated as theorems with a citation.
    pub fn env(self) -> &'static str {
        match self {
            Kind::Theorem | Kind::Known => "theorem",
            Kind::Proposition => "proposition",
            Kind::Lemma => "lemma",
            Kind::Corollary => "corollary",
            Kind::Definition => "definition",
            Kind::Remark => "remark",
            Kind::Example => "example",
        }
    }

    /// Name used when the statement is printed in the paper.
    pub fn printed(self) -> &'static str {
        match self {
            Kind::Known => "Theorem",
            k => k.name(),
        }
    }

    pub fn needs_proof(self) -> bool {
        matches!(
            self,
            Kind::Theorem | Kind::Proposition | Kind::Lemma | Kind::Corollary
        )
    }

    /// Whether it makes sense to ask "what does this depend on?".
    pub fn expandable(self) -> bool {
        self != Kind::Known
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Its dependencies have not been asked for yet.
    #[default]
    Open,
    /// Its dependencies are listed.
    Expanded,
    /// Needs nothing beyond common knowledge: stop here.
    Basic,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub statement: String,
    /// One of the results that justify the paper.
    #[serde(default, skip_serializing_if = "is_false")]
    pub main: bool,
    #[serde(default)]
    pub status: Status,
    /// Ids of the statements this one uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    /// Source of a known result ("Theorem 2 of [5]").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cite: String,
    /// The proof is deferred to the appendix.
    #[serde(default, skip_serializing_if = "is_false")]
    pub appendix: bool,
    /// Notation the statement mentions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// LaTeX for the symbol, e.g. `G` or `\mathcal{F}`.
    pub notation: String,
    pub meaning: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub family: String,
    /// Id of the statement that introduces it; empty for general notation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub introduced_by: String,
    /// Optional macro name for macros.tex, e.g. `\graph`.
    #[serde(default, rename = "macro", skip_serializing_if = "String::is_empty")]
    pub macro_name: String,
}

/// A family of letters reserved for one kind of object (Halmos: decide the alphabet first).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Family {
    pub name: String,
    pub letters: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaperInfo {
    #[serde(default)]
    pub title: String,
    /// The story in one paragraph: problem, why it matters, what is proved, which technique.
    #[serde(default)]
    pub story: String,
    /// TCS section the paper is aimed at.
    #[serde(default)]
    pub section: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Paper {
    #[serde(default)]
    pub paper: PaperInfo,
    #[serde(default, rename = "family")]
    pub families: Vec<Family>,
    #[serde(default, rename = "symbol")]
    pub symbols: Vec<Symbol>,
    #[serde(default, rename = "node")]
    pub nodes: Vec<Node>,
}

pub const TCS_SECTIONS: [&str; 3] = [
    "Algorithms, automata, complexity and games",
    "Logic, semantics and theory of programming",
    "Natural Computing",
];

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out.trim_end_matches('-').chars().take(32).collect();
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
}

pub fn default_families() -> Vec<Family> {
    let fam = |name: &str, letters: &[&str]| Family {
        name: name.into(),
        letters: letters.iter().map(|s| s.to_string()).collect(),
    };
    vec![
        fam("integer", &["n", "m", "k", "i", "j", "\\ell"]),
        fam("real", &["x", "y", "z", "t"]),
        fam("small", &["\\varepsilon", "\\delta"]),
        fam("constant", &["c", "\\alpha", "\\beta"]),
        fam("set", &["A", "B", "S", "T", "X", "Y"]),
        fam("family", &["\\mathcal{F}", "\\mathcal{A}", "\\mathcal{S}"]),
        fam("function", &["f", "g", "h", "\\varphi"]),
        fam("graph", &["G", "H"]),
        fam("vertex", &["u", "v", "w"]),
    ]
}

impl Paper {
    pub fn new() -> Self {
        Paper {
            families: default_families(),
            ..Default::default()
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&s).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, s).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
    }

    pub fn index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// "Lemma “Degree bound”", or the raw id if it does not exist.
    pub fn label(&self, id: &str) -> String {
        match self.node(id) {
            Some(n) => format!("{} “{}”", n.kind.name(), n.title),
            None => format!("‹{id}›"),
        }
    }

    pub fn fresh_id(&self, kind: Kind, title: &str) -> String {
        let s = slug(title);
        let base = format!("{}:{}", kind.prefix(), if s.is_empty() { "x" } else { &s });
        let mut id = base.clone();
        let mut k = 2;
        while self.index(&id).is_some() {
            id = format!("{base}-{k}");
            k += 1;
        }
        id
    }

    pub fn add_node(&mut self, kind: Kind, title: &str) -> String {
        let id = self.fresh_id(kind, title);
        self.nodes.push(Node {
            id: id.clone(),
            kind,
            title: title.to_string(),
            statement: String::new(),
            main: false,
            status: if kind.expandable() {
                Status::Open
            } else {
                Status::Basic
            },
            uses: vec![],
            cite: String::new(),
            appendix: false,
            symbols: vec![],
        });
        id
    }

    pub fn remove_node(&mut self, id: &str) {
        self.nodes.retain(|n| n.id != id);
        for n in &mut self.nodes {
            n.uses.retain(|u| u != id);
        }
        for s in &mut self.symbols {
            if s.introduced_by == id {
                s.introduced_by.clear();
            }
        }
    }

    pub fn rename_node(&mut self, old: &str, new: &str) -> Result<(), String> {
        if old == new {
            return Ok(());
        }
        if !valid_id(new) {
            return Err(format!(
                "“{new}” is not a valid id (use letters, digits, : - _ .)"
            ));
        }
        if self.index(new).is_some() {
            return Err(format!("id “{new}” is already taken"));
        }
        for n in &mut self.nodes {
            if n.id == old {
                n.id = new.to_string();
            }
            for u in &mut n.uses {
                if u == old {
                    *u = new.to_string();
                }
            }
        }
        for s in &mut self.symbols {
            if s.introduced_by == old {
                s.introduced_by = new.to_string();
            }
        }
        Ok(())
    }

    /// Change the kind; an id with the old kind's prefix gets the new one. Returns the id.
    pub fn set_kind(&mut self, id: &str, kind: Kind) -> String {
        let Some(n) = self.node_mut(id) else {
            return id.to_string();
        };
        let old = n.kind;
        n.kind = kind;
        if !kind.expandable() {
            n.status = Status::Basic;
        }
        let title = n.title.clone();
        match id
            .strip_prefix(old.prefix())
            .and_then(|t| t.strip_prefix(':'))
        {
            Some(tail) if old.prefix() != kind.prefix() => {
                let mut new = format!("{}:{tail}", kind.prefix());
                if self.index(&new).is_some() {
                    new = self.fresh_id(kind, &title);
                }
                self.rename_node(id, &new).expect("fresh id");
                new
            }
            _ => id.to_string(),
        }
    }

    /// Record that `from` uses `to`. Refuses self-loops and edges that close a cycle.
    pub fn link(&mut self, from: &str, to: &str) -> Result<(), String> {
        if from == to {
            return Err("a statement cannot use itself".into());
        }
        if self.depends_on(to, from) {
            return Err(format!(
                "{} already depends on {}: that would be a cycle",
                self.label(to),
                self.label(from)
            ));
        }
        let n = self.node_mut(from).ok_or("no such node")?;
        if !n.uses.iter().any(|u| u == to) {
            n.uses.push(to.to_string());
        }
        Ok(())
    }

    pub fn unlink(&mut self, from: &str, to: &str) {
        if let Some(n) = self.node_mut(from) {
            n.uses.retain(|u| u != to);
        }
    }

    pub fn users_of(&self, id: &str) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|n| n.uses.iter().any(|u| u == id))
            .collect()
    }

    /// Whether `from` uses `to`, directly or through other statements.
    pub fn depends_on(&self, from: &str, to: &str) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(n) = self.node(id) {
                for u in &n.uses {
                    if u == to {
                        return true;
                    }
                    stack.push(u);
                }
            }
        }
        false
    }

    /// Statements still waiting for "what does it depend on?", top-down from the main results.
    pub fn open_queue(&self) -> Vec<String> {
        let wants = |n: &Node| n.status == Status::Open && n.kind.expandable();
        let mut seen = HashSet::new();
        let mut queue: VecDeque<&str> = self
            .nodes
            .iter()
            .filter(|n| n.main)
            .map(|n| n.id.as_str())
            .collect();
        let mut out = vec![];
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            let Some(n) = self.node(id) else { continue };
            if wants(n) {
                out.push(id.to_string());
            }
            queue.extend(n.uses.iter().map(String::as_str));
        }
        out.extend(
            self.nodes
                .iter()
                .filter(|n| !seen.contains(n.id.as_str()) && wants(n))
                .map(|n| n.id.clone()),
        );
        out
    }

    pub fn family(&self, name: &str) -> Option<&Family> {
        self.families.iter().find(|f| f.name == name)
    }

    pub fn symbol(&self, notation: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.notation == notation)
    }

    /// First letter of the family that no symbol uses yet.
    pub fn suggest_letter(&self, family: &str) -> Option<String> {
        self.family(family)?
            .letters
            .iter()
            .find(|l| self.symbol(l).is_none())
            .cloned()
    }
}

/// Split "G, H  \mathcal{F}" into letters.
pub fn parse_list(s: &str) -> Vec<String> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_ids() {
        assert_eq!(slug("Degree bound for G!"), "degree-bound-for-g");
        let mut p = Paper::new();
        let a = p.add_node(Kind::Lemma, "Degree bound");
        let b = p.add_node(Kind::Lemma, "Degree bound");
        assert_eq!(a, "lem:degree-bound");
        assert_eq!(b, "lem:degree-bound-2");
    }

    #[test]
    fn link_refuses_cycles() {
        let mut p = Paper::new();
        let a = p.add_node(Kind::Theorem, "A");
        let b = p.add_node(Kind::Lemma, "B");
        let c = p.add_node(Kind::Lemma, "C");
        p.link(&a, &b).unwrap();
        p.link(&b, &c).unwrap();
        assert!(p.link(&c, &a).is_err());
        assert!(p.link(&a, &a).is_err());
        assert!(p.depends_on(&a, &c));
    }

    #[test]
    fn rename_updates_references() {
        let mut p = Paper::new();
        let a = p.add_node(Kind::Theorem, "A");
        let b = p.add_node(Kind::Lemma, "B");
        p.link(&a, &b).unwrap();
        p.rename_node(&b, "lem:key").unwrap();
        assert_eq!(p.node(&a).unwrap().uses, vec!["lem:key"]);
    }

    #[test]
    fn round_trip() {
        let mut p = Paper::new();
        let a = p.add_node(Kind::Theorem, "A");
        p.node_mut(&a).unwrap().main = true;
        let s = toml::to_string_pretty(&p).unwrap();
        let q: Paper = toml::from_str(&s).unwrap();
        assert_eq!(q.nodes.len(), 1);
        assert!(q.nodes[0].main);
    }
}
