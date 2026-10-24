use super::{Block, Function};

/// reverse postorder, predecessors and dominators over the blocks reachable from the entry
pub struct ControlFlow {
    rpo: Vec<Block>,
    rpo_num: Vec<Option<usize>>,
    preds: Vec<Vec<Block>>,
    idom: Vec<Option<Block>>,
}

impl ControlFlow {
    pub fn compute(func: &Function) -> Self {
        let n = func.blocks().count();
        let successors = |b: Block| -> Vec<Block> {
            func.block(b)
                .terminator()
                .map(|t| {
                    t.targets()
                        .into_iter()
                        .map(|(dest, _)| dest)
                        .filter(|&dest| func.contains_block(dest))
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut postorder = Vec::new();
        let mut visited = vec![false; n];
        let mut stack = vec![(func.entry_block(), false)];
        while let Some((b, done)) = stack.pop() {
            if done {
                postorder.push(b);
                continue;
            }
            if std::mem::replace(&mut visited[b.index()], true) {
                continue;
            }
            stack.push((b, true));
            stack.extend(
                successors(b)
                    .into_iter()
                    .filter(|s| !visited[s.index()])
                    .map(|s| (s, false)),
            );
        }
        let rpo: Vec<Block> = postorder.into_iter().rev().collect();
        let mut rpo_num = vec![None; n];
        for (i, &b) in rpo.iter().enumerate() {
            rpo_num[b.index()] = Some(i);
        }

        let mut preds = vec![Vec::new(); n];
        for &b in &rpo {
            for s in successors(b) {
                preds[s.index()].push(b);
            }
        }

        // cooper-harvey-kennedy
        let mut idom: Vec<Option<Block>> = vec![None; n];
        idom[func.entry_block().index()] = Some(func.entry_block());
        let num = |b: Block| rpo_num[b.index()].expect("reachable");
        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                let mut candidates = preds[b.index()]
                    .iter()
                    .copied()
                    .filter(|p| idom[p.index()].is_some());
                let Some(first) = candidates.next() else {
                    continue;
                };
                let new_idom = candidates.fold(first, |a, p| {
                    let (mut a, mut p) = (a, p);
                    while a != p {
                        while num(a) > num(p) {
                            a = idom[a.index()].unwrap();
                        }
                        while num(p) > num(a) {
                            p = idom[p.index()].unwrap();
                        }
                    }
                    a
                });
                if idom[b.index()] != Some(new_idom) {
                    idom[b.index()] = Some(new_idom);
                    changed = true;
                }
            }
        }

        Self {
            rpo,
            rpo_num,
            preds,
            idom,
        }
    }

    pub fn reverse_postorder(&self) -> &[Block] {
        &self.rpo
    }

    pub fn is_reachable(&self, block: Block) -> bool {
        self.rpo_num[block.index()].is_some()
    }

    /// reachable predecessors, once per edge
    pub fn predecessors(&self, block: Block) -> &[Block] {
        &self.preds[block.index()]
    }

    /// the immediate dominator; `None` for the entry and unreachable blocks
    pub fn idom(&self, block: Block) -> Option<Block> {
        self.idom[block.index()].filter(|&parent| parent != block)
    }

    pub fn dominates(&self, a: Block, b: Block) -> bool {
        let mut b = b;
        loop {
            if a == b {
                return true;
            }
            match self.idom(b) {
                Some(parent) => b = parent,
                None => return false,
            }
        }
    }

    /// an edge to a block at or before its source in reverse postorder
    pub fn is_retreating(&self, from: Block, to: Block) -> bool {
        self.rpo_num[to.index()] <= self.rpo_num[from.index()]
    }

    pub fn is_reducible(&self) -> bool {
        self.rpo.iter().all(|&to| {
            self.predecessors(to)
                .iter()
                .all(|&from| !self.is_retreating(from, to) || self.dominates(to, from))
        })
    }
}
