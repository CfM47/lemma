//! Diagnostics for the dependency graph and the alphabet.

use crate::graph::Graph;
use crate::model::{Kind, Paper};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
}

pub fn run(p: &Paper) -> Vec<Diagnostic> {
    let mut out = vec![];
    let mut say = |level, message: String| out.push(Diagnostic { level, message });

    // Ids
    let mut ids = HashSet::new();
    for n in &p.nodes {
        if !ids.insert(n.id.as_str()) {
            say(
                Level::Error,
                format!("id “{}” is used by two statements", n.id),
            );
        }
    }
    for n in &p.nodes {
        for u in &n.uses {
            if u == &n.id {
                say(Level::Error, format!("{} uses itself", p.label(&n.id)));
            } else if p.node(u).is_none() {
                say(
                    Level::Error,
                    format!("{} uses “{u}”, which does not exist", p.label(&n.id)),
                );
            }
        }
    }

    // Graph
    let g = Graph::new(p);
    if let Some(cycle) = g.find_cycle() {
        let mut names: Vec<String> = cycle.iter().map(|&v| p.label(&p.nodes[v].id)).collect();
        names.push(names[0].clone());
        say(
            Level::Error,
            format!("circular reasoning: {}", names.join(" → ")),
        );
    }

    let mains: Vec<_> = p.nodes.iter().filter(|n| n.main).collect();
    if mains.is_empty() && !p.nodes.is_empty() {
        say(
            Level::Warning,
            "no main result: mark the theorems that justify the paper".into(),
        );
    }
    if mains.len() > 3 {
        say(
            Level::Info,
            format!(
                "{} main results: 1 to 3 is usually best; demote the rest to propositions",
                mains.len()
            ),
        );
    }
    for n in &mains {
        if !matches!(n.kind, Kind::Theorem | Kind::Proposition) {
            say(
                Level::Warning,
                format!("{} is marked main but is not a theorem", p.label(&n.id)),
            );
        }
    }
    for (v, n) in p.nodes.iter().enumerate() {
        let unused = !n.main
            && g.users[v].is_empty()
            && !matches!(n.kind, Kind::Corollary | Kind::Remark | Kind::Example);
        if unused {
            say(
                Level::Warning,
                format!(
                    "{} is not used by anything: remove it or turn it into a remark",
                    p.label(&n.id)
                ),
            );
        }
        if n.kind == Kind::Known && n.cite.trim().is_empty() {
            say(
                Level::Warning,
                format!("{} has no citation", p.label(&n.id)),
            );
        }
        if n.kind == Kind::Corollary && g.uses[v].is_empty() {
            say(
                Level::Warning,
                format!("{} is a corollary of nothing", p.label(&n.id)),
            );
        }
        if n.appendix && !n.kind.needs_proof() {
            say(
                Level::Info,
                format!("{} has no proof to defer to the appendix", p.label(&n.id)),
            );
        }
    }
    let open = p.open_queue();
    if !open.is_empty() {
        let names: Vec<String> = open.iter().map(|id| p.label(id)).collect();
        say(
            Level::Info,
            format!("{} still open: {}", open.len(), names.join(", ")),
        );
    }
    // Alphabet
    let mut meaning: HashMap<&str, &str> = HashMap::new();
    for s in &p.symbols {
        if let Some(prev) = meaning.insert(&s.notation, &s.meaning) {
            say(
                Level::Error,
                format!(
                    "${}$ means both “{prev}” and “{}”: one letter, one meaning",
                    s.notation, s.meaning
                ),
            );
        }
        if !s.introduced_by.is_empty() && p.node(&s.introduced_by).is_none() {
            say(
                Level::Error,
                format!(
                    "${}$ is introduced by “{}”, which does not exist",
                    s.notation, s.introduced_by
                ),
            );
        }
        if s.family.is_empty() {
            continue;
        }
        match p.family(&s.family) {
            None => say(
                Level::Warning,
                format!("${}$ belongs to unknown family “{}”", s.notation, s.family),
            ),
            Some(f) if !f.letters.iter().any(|l| decorates(&s.notation, l)) => {
                let owner = p
                    .families
                    .iter()
                    .find(|f| f.letters.iter().any(|l| decorates(&s.notation, l)));
                let why = match owner {
                    Some(o) => format!(", and it is reserved for {}", o.name),
                    None => String::new(),
                };
                say(
                    Level::Warning,
                    format!(
                        "${}$ ({}) is not one of the {} letters {}{why}",
                        s.notation,
                        s.meaning,
                        f.name,
                        f.letters.join(" ")
                    ),
                );
            }
            Some(_) => {}
        }
    }
    let mut owner: HashMap<&str, &str> = HashMap::new();
    for f in &p.families {
        for l in &f.letters {
            if let Some(prev) = owner.insert(l, &f.name) {
                say(
                    Level::Warning,
                    format!("${l}$ is reserved for both {prev} and {}", f.name),
                );
            }
        }
    }
    for n in &p.nodes {
        for sym in &n.symbols {
            match p.symbol(sym) {
                None => say(
                    Level::Warning,
                    format!(
                        "{} mentions ${sym}$, which is not in the alphabet",
                        p.label(&n.id)
                    ),
                ),
                Some(s)
                    if !s.introduced_by.is_empty()
                        && s.introduced_by != n.id
                        && !p.depends_on(&n.id, &s.introduced_by) =>
                {
                    say(
                        Level::Warning,
                        format!(
                            "{} mentions ${sym}$ but does not depend on {}, which introduces it",
                            p.label(&n.id),
                            p.label(&s.introduced_by)
                        ),
                    )
                }
                Some(_) => {}
            }
        }
    }

    out.sort_by_key(|d| d.level);
    out
}

/// `G`, `G'`, `G_1` and `G^*` all belong to the letter `G`.
pub fn decorates(notation: &str, letter: &str) -> bool {
    match notation.strip_prefix(letter) {
        Some("") => true,
        Some(rest) => rest.starts_with(['_', '^', '\'']),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Symbol;

    fn sym(notation: &str, meaning: &str, family: &str) -> Symbol {
        Symbol {
            notation: notation.into(),
            meaning: meaning.into(),
            family: family.into(),
            introduced_by: String::new(),
            macro_name: String::new(),
        }
    }

    #[test]
    fn alphabet_clashes() {
        let mut p = Paper::new();
        p.symbols.push(sym("G", "input graph", "graph"));
        p.symbols.push(sym("G", "a group", "graph"));
        p.symbols.push(sym("n", "number of vertices", "set"));
        p.symbols.push(sym("H_1", "subgraph", "graph"));
        let d = run(&p);
        assert!(
            d.iter()
                .any(|d| d.level == Level::Error && d.message.contains("means both"))
        );
        assert!(d.iter().any(|d| d.message.contains("reserved for integer")));
        assert!(!d.iter().any(|d| d.message.contains("H_1")));
    }

    #[test]
    fn symbol_before_definition() {
        let mut p = Paper::new();
        let def = p.add_node(Kind::Definition, "Graph");
        let lem = p.add_node(Kind::Lemma, "L");
        let mut s = sym("G", "graph", "graph");
        s.introduced_by = def.clone();
        p.symbols.push(s);
        p.node_mut(&lem).unwrap().symbols.push("G".into());
        assert!(
            run(&p)
                .iter()
                .any(|d| d.message.contains("does not depend on"))
        );
        p.link(&lem, &def).unwrap();
        assert!(
            !run(&p)
                .iter()
                .any(|d| d.message.contains("does not depend on"))
        );
    }
}
