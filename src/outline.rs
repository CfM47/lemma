//! From the dependency graph to an ordered outline.
//!
//! Rules (see the paper's guide, sections 1 and 2):
//! - one section per main result, and one per *technique*: a lemma that sits
//!   right below a main result and on which other results depend only through it
//!   (in graph terms, a lemma that dominates other results);
//! - a statement that only matters for one section (it is dominated by that
//!   section's head) goes in that section; a lemma shared by several sections
//!   goes in the first section that uses it;
//! - known results, and definitions used in more than one section, go to
//!   Preliminaries; a definition used in one section only is stated there;
//! - nothing comes after something that uses it;
//! - inside a section, dependencies come right before their first use, and a
//!   corollary right after the last thing it needs.

use crate::graph::Graph;
use crate::model::{Kind, Paper, slug};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Preliminaries,
    /// Headed by a main result (node index).
    Main(usize),
    /// Headed by a key lemma behind a main result (node index).
    Technique(usize),
}

#[derive(Debug, Clone)]
pub struct Section {
    pub kind: SectionKind,
    /// Section number in the paper (1 is the introduction).
    pub number: usize,
    pub title: String,
    pub label: String,
    pub items: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Outline {
    pub sections: Vec<Section>,
    /// Reading order of all statements.
    pub order: Vec<usize>,
    /// Printed number of each statement, e.g. "3.2".
    pub numbers: Vec<String>,
    /// Index into `sections` for each statement.
    pub section_of: Vec<usize>,
    /// Main results in reading order.
    pub mains: Vec<usize>,
    /// Statements whose proof is deferred to the appendix, in reading order.
    pub appendix: Vec<usize>,
}

impl Outline {
    pub fn build(p: &Paper) -> Result<Outline, Vec<usize>> {
        let g = Graph::new(p);
        if let Some(cycle) = g.find_cycle() {
            return Err(cycle);
        }
        let n = p.nodes.len();
        let kind = |v: usize| p.nodes[v].kind;

        // Entry points: the main results, then whatever nobody uses.
        let mut roots: Vec<usize> = (0..n).filter(|&v| p.nodes[v].main).collect();
        roots.extend((0..n).filter(|&v| !p.nodes[v].main && g.users[v].is_empty()));

        let mut order = vec![];
        let mut emitted = vec![false; n];
        for &r in &roots {
            emit(r, &g, p, &mut emitted, &mut order);
        }
        let mut pos = vec![0; n];
        for (i, &v) in order.iter().enumerate() {
            pos[v] = i;
        }

        let mut mains: Vec<usize> = (0..n).filter(|&v| p.nodes[v].main).collect();
        mains.sort_by_key(|&v| pos[v]);
        // Without main results, lay the paper out around whatever results nobody uses.
        let tops: Vec<usize> = if mains.is_empty() {
            roots
                .iter()
                .copied()
                .filter(|&v| kind(v).needs_proof())
                .collect()
        } else {
            mains.clone()
        };

        let idom = g.dominators(&roots);
        let mut dominates_result = vec![false; n];
        for w in (0..n).filter(|&w| kind(w).needs_proof()) {
            let mut x = idom[w];
            while x != n {
                dominates_result[x] = true;
                x = idom[x];
            }
        }
        let mut heads: Vec<usize> = (0..n)
            .filter(|&v| {
                tops.contains(&v)
                    || (kind(v).needs_proof() && tops.contains(&idom[v]) && dominates_result[v])
            })
            .collect();
        heads.sort_by_key(|&v| pos[v]);

        // Slots: 0 is Preliminaries, k is the section of heads[k - 1].
        let slot_of_head: HashMap<usize, usize> =
            heads.iter().enumerate().map(|(k, &h)| (h, k + 1)).collect();
        let last = heads.len();
        let nearest_head = |v: usize| {
            let mut x = v;
            while x != n {
                if let Some(&s) = slot_of_head.get(&x) {
                    return Some(s);
                }
                x = idom[x];
            }
            None
        };

        // Users first: place each statement no later than its first user.
        let mut slot = vec![0; n];
        for &v in order.iter().rev() {
            let user_slots: Vec<usize> = g.users[v].iter().map(|&u| slot[u]).collect();
            slot[v] = match kind(v) {
                Kind::Known => 0,
                Kind::Definition => match user_slots.first() {
                    Some(&s) if user_slots.iter().all(|&x| x == s) => s,
                    _ => 0,
                },
                k => match (nearest_head(v), user_slots.iter().copied().min()) {
                    (Some(h), Some(u)) => h.min(u),
                    (Some(h), None) => h,
                    (None, Some(u)) => u,
                    (None, None) if matches!(k, Kind::Remark | Kind::Example) => 0,
                    (None, None) => last,
                },
            };
        }
        // Then never before what it uses; what nobody uses goes right after its last dependency.
        for &v in &order {
            let dep = g.uses[v].iter().map(|&u| slot[u]).max();
            if let Some(d) = dep {
                slot[v] = if g.users[v].is_empty() && !slot_of_head.contains_key(&v) {
                    d
                } else {
                    slot[v].max(d)
                };
            }
        }

        let mut sections: Vec<Section> = vec![];
        let mut section_of = vec![0; n];
        let mut numbers = vec![String::new(); n];
        for s in 0..=last {
            let items: Vec<usize> = order.iter().copied().filter(|&v| slot[v] == s).collect();
            if items.is_empty() {
                continue;
            }
            let number = sections.len() + 2;
            let (kind, title, label) = if s == 0 {
                (
                    SectionKind::Preliminaries,
                    "Preliminaries".to_string(),
                    "sec:prelim".to_string(),
                )
            } else {
                let h = heads[s - 1];
                let node = &p.nodes[h];
                let tail = node.id.split_once(':').map_or(node.id.as_str(), |(_, t)| t);
                let tail = if tail.is_empty() {
                    slug(&node.title)
                } else {
                    tail.to_string()
                };
                let kind = if node.main {
                    SectionKind::Main(h)
                } else {
                    SectionKind::Technique(h)
                };
                (kind, node.title.clone(), format!("sec:{tail}"))
            };
            for (k, &v) in items.iter().enumerate() {
                section_of[v] = sections.len();
                numbers[v] = format!("{number}.{}", k + 1);
            }
            sections.push(Section {
                kind,
                number,
                title,
                label,
                items,
            });
        }

        let appendix = order
            .iter()
            .copied()
            .filter(|&v| p.nodes[v].appendix && kind(v).needs_proof())
            .collect();

        Ok(Outline {
            sections,
            order,
            numbers,
            section_of,
            mains,
            appendix,
        })
    }

    /// "Lemma 3.2"
    pub fn reference(&self, p: &Paper, v: usize) -> String {
        format!("{} {}", p.nodes[v].kind.printed(), self.numbers[v])
    }
}

/// Post-order: dependencies first, in the order they are listed.
fn emit(v: usize, g: &Graph, p: &Paper, emitted: &mut [bool], order: &mut Vec<usize>) {
    if emitted[v] {
        return;
    }
    for &u in &g.uses[v] {
        emit(u, g, p, emitted, order);
    }
    emitted[v] = true;
    order.push(v);
    pull_corollaries(v, g, p, emitted, order);
}

fn pull_corollaries(v: usize, g: &Graph, p: &Paper, emitted: &mut [bool], order: &mut Vec<usize>) {
    for &c in &g.users[v] {
        if !emitted[c]
            && p.nodes[c].kind == Kind::Corollary
            && g.uses[c].iter().all(|&u| emitted[u])
        {
            emitted[c] = true;
            order.push(c);
            pull_corollaries(c, g, p, emitted, order);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper() -> Paper {
        // Mains First and Second; Second uses First. Shared and DS are used under both,
        // L1 and D1 only under First, L2 only under Second. C is a corollary of First.
        let mut p = Paper::new();
        let t1 = p.add_node(Kind::Theorem, "First");
        let t2 = p.add_node(Kind::Theorem, "Second");
        let l1 = p.add_node(Kind::Lemma, "L1");
        let l2 = p.add_node(Kind::Lemma, "L2");
        let ls = p.add_node(Kind::Lemma, "Shared");
        let d1 = p.add_node(Kind::Definition, "D1");
        let ds = p.add_node(Kind::Definition, "DS");
        let k = p.add_node(Kind::Known, "K");
        let c = p.add_node(Kind::Corollary, "C");
        p.node_mut(&t1).unwrap().main = true;
        p.node_mut(&t2).unwrap().main = true;
        for (a, b) in [
            (&t1, &l1),
            (&t1, &d1),
            (&t1, &k),
            (&t2, &t1),
            (&t2, &l2),
            (&l1, &ls),
            (&l2, &ls),
            (&l1, &ds),
            (&l2, &ds),
            (&c, &t1),
        ] {
            p.link(a, b).unwrap();
        }
        p
    }

    fn title_of(p: &Paper, v: usize) -> &str {
        &p.nodes[v].title
    }

    #[test]
    fn sections_follow_dominators() {
        let p = paper();
        let o = Outline::build(&p).unwrap();
        let titles: Vec<(String, Vec<&str>)> = o
            .sections
            .iter()
            .map(|s| {
                (
                    s.title.clone(),
                    s.items.iter().map(|&v| title_of(&p, v)).collect(),
                )
            })
            .collect();
        assert_eq!(
            titles,
            vec![
                ("Preliminaries".into(), vec!["DS", "K"]),
                ("First".into(), vec!["Shared", "L1", "D1", "First", "C"]),
                ("Second".into(), vec!["L2", "Second"]),
            ]
        );
        assert_eq!(o.numbers[0], "3.4");
        assert_eq!(o.sections[0].number, 2);
    }

    #[test]
    fn key_lemma_gets_its_own_section() {
        // One main theorem T uses key lemma K and definition D. K uses helper H, H uses D.
        let mut p = Paper::new();
        let t = p.add_node(Kind::Theorem, "T");
        let k = p.add_node(Kind::Lemma, "K");
        let h = p.add_node(Kind::Lemma, "H");
        let d = p.add_node(Kind::Definition, "D");
        p.node_mut(&t).unwrap().main = true;
        for (a, b) in [(&t, &k), (&t, &d), (&k, &h), (&h, &d)] {
            p.link(a, b).unwrap();
        }
        let o = Outline::build(&p).unwrap();
        let titles: Vec<(String, Vec<&str>)> = o
            .sections
            .iter()
            .map(|s| {
                (
                    s.title.clone(),
                    s.items.iter().map(|&v| title_of(&p, v)).collect(),
                )
            })
            .collect();
        assert_eq!(
            titles,
            vec![
                ("Preliminaries".into(), vec!["D"]),
                ("K".into(), vec!["H", "K"]),
                ("T".into(), vec!["T"]),
            ]
        );
        assert!(matches!(o.sections[1].kind, SectionKind::Technique(_)));
    }

    #[test]
    fn order_respects_dependencies() {
        let p = paper();
        let o = Outline::build(&p).unwrap();
        let g = Graph::new(&p);
        let rank = |v: usize| {
            (
                o.section_of[v],
                o.order.iter().position(|&x| x == v).unwrap(),
            )
        };
        for v in 0..p.nodes.len() {
            for &u in &g.uses[v] {
                assert!(
                    rank(u) < rank(v),
                    "{} before {}",
                    title_of(&p, u),
                    title_of(&p, v)
                );
            }
        }
    }

    #[test]
    fn cycle_is_an_error() {
        let mut p = paper();
        let first = p.nodes[0].id.clone();
        p.nodes[2].uses.push(first);
        assert!(Outline::build(&p).is_err());
    }
}
