//! The dependency graph by index: cycles and dominators.

use crate::model::Paper;
use std::collections::HashMap;

pub struct Graph {
    pub n: usize,
    /// `uses[v]`: statements `v` uses.
    pub uses: Vec<Vec<usize>>,
    /// `users[v]`: statements that use `v`.
    pub users: Vec<Vec<usize>>,
}

const UNDEF: usize = usize::MAX;

impl Graph {
    /// Builds the graph. References to unknown ids are skipped (`check` reports them).
    pub fn new(p: &Paper) -> Self {
        let n = p.nodes.len();
        let idx: HashMap<&str, usize> = p
            .nodes
            .iter()
            .enumerate()
            .map(|(i, x)| (x.id.as_str(), i))
            .collect();
        let mut uses = vec![vec![]; n];
        let mut users = vec![vec![]; n];
        for (i, node) in p.nodes.iter().enumerate() {
            for u in &node.uses {
                if let Some(&j) = idx.get(u.as_str())
                    && !uses[i].contains(&j)
                {
                    uses[i].push(j);
                    users[j].push(i);
                }
            }
        }
        Graph { n, uses, users }
    }

    /// Some cycle `v0 -> v1 -> ... -> v0`, if there is one.
    pub fn find_cycle(&self) -> Option<Vec<usize>> {
        fn dfs(
            g: &Graph,
            v: usize,
            color: &mut [u8],
            stack: &mut Vec<usize>,
        ) -> Option<Vec<usize>> {
            color[v] = 1;
            stack.push(v);
            for &u in &g.uses[v] {
                if color[u] == 1 {
                    let start = stack.iter().position(|&x| x == u).unwrap();
                    return Some(stack[start..].to_vec());
                }
                if color[u] == 0
                    && let Some(c) = dfs(g, u, color, stack)
                {
                    return Some(c);
                }
            }
            stack.pop();
            color[v] = 2;
            None
        }
        let mut color = vec![0u8; self.n];
        (0..self.n).find_map(|v| {
            if color[v] == 0 {
                dfs(self, v, &mut color, &mut vec![])
            } else {
                None
            }
        })
    }

    /// Immediate dominators, with a virtual root (index `n`) that uses `roots`.
    ///
    /// `v` dominates `w` when every chain of "uses" from the root down to `w` passes
    /// through `v`: then `w` only matters for `v`, and belongs next to it.
    /// Cooper, Harvey and Kennedy, "A simple, fast dominance algorithm".
    pub fn dominators(&self, roots: &[usize]) -> Vec<usize> {
        let root = self.n;
        let succ = |v: usize| -> &[usize] { if v == root { roots } else { &self.uses[v] } };

        let mut po = vec![UNDEF; self.n + 1];
        let mut post = vec![];
        let mut visited = vec![false; self.n + 1];
        // Iterative DFS so deep chains do not overflow the stack.
        let mut stack = vec![(root, 0usize)];
        visited[root] = true;
        while let Some(&(v, i)) = stack.last() {
            if let Some(&u) = succ(v).get(i) {
                stack.last_mut().unwrap().1 += 1;
                if !visited[u] {
                    visited[u] = true;
                    stack.push((u, 0));
                }
            } else {
                po[v] = post.len();
                post.push(v);
                stack.pop();
            }
        }

        let preds = |v: usize| {
            let mut p = self.users[v].clone();
            if roots.contains(&v) {
                p.push(root);
            }
            p
        };
        let mut idom = vec![UNDEF; self.n + 1];
        idom[root] = root;
        let intersect = |idom: &[usize], mut a: usize, mut b: usize| {
            while a != b {
                while po[a] < po[b] {
                    a = idom[a];
                }
                while po[b] < po[a] {
                    b = idom[b];
                }
            }
            a
        };
        loop {
            let mut changed = false;
            for &b in post.iter().rev().skip(1) {
                let mut new = UNDEF;
                for p in preds(b) {
                    if idom[p] != UNDEF {
                        new = if new == UNDEF {
                            p
                        } else {
                            intersect(&idom, p, new)
                        };
                    }
                }
                if new != UNDEF && idom[b] != new {
                    idom[b] = new;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        idom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Paper};

    #[test]
    fn diamond_dominators() {
        // a uses b, c; b and c use d. a dominates everything, d is dominated by a only.
        let mut p = Paper::new();
        let a = p.add_node(Kind::Theorem, "a");
        let b = p.add_node(Kind::Lemma, "b");
        let c = p.add_node(Kind::Lemma, "c");
        let d = p.add_node(Kind::Lemma, "d");
        p.link(&a, &b).unwrap();
        p.link(&a, &c).unwrap();
        p.link(&b, &d).unwrap();
        p.link(&c, &d).unwrap();
        let g = Graph::new(&p);
        let idom = g.dominators(&[0]);
        assert_eq!(idom[1], 0);
        assert_eq!(idom[2], 0);
        assert_eq!(idom[3], 0);
        assert_eq!(idom[0], 4);
    }

    #[test]
    fn finds_cycle() {
        let mut p = Paper::new();
        let a = p.add_node(Kind::Lemma, "a");
        let b = p.add_node(Kind::Lemma, "b");
        p.node_mut(&a).unwrap().uses.push(b.clone());
        p.node_mut(&b).unwrap().uses.push(a.clone());
        assert_eq!(Graph::new(&p).find_cycle().map(|c| c.len()), Some(2));
    }
}
