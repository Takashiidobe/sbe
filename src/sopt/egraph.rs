use std::collections::HashMap;

use crate::sir::{BinOp, Block, ControlFlow, Function, InstData, Type, Value, ValueDef};

use super::{ConstantFold, Pass};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Input {
        ty: Type,
        value: Value,
    },
    Iconst {
        ty: Type,
        value: u64,
    },
    Binary {
        ty: Type,
        op: BinOp,
        args: [NodeId; 2],
    },
}

impl Expr {
    pub fn ty(&self) -> Type {
        match self {
            Self::Input { ty, .. } | Self::Iconst { ty, .. } | Self::Binary { ty, .. } => *ty,
        }
    }

    fn args(&self) -> &[NodeId] {
        match self {
            Self::Binary { args, .. } => args,
            _ => &[],
        }
    }
}

pub trait Rewrite {
    fn priority(&self) -> u32;
    fn accepts(&self, expr: &Expr) -> bool;
    fn rewrite(&self, graph: &EGraph, expr: &Expr) -> Option<Expr>;
}

struct Node {
    expr: Expr,
    alternatives: Vec<NodeId>,
    best: NodeId,
    cost: u64,
}

#[derive(Default)]
pub struct EGraph {
    nodes: Vec<Node>,
    interned: HashMap<Expr, NodeId>,
}

impl EGraph {
    pub fn expr(&self, node: NodeId) -> &Expr {
        &self.nodes[self.best(node).0].expr
    }

    pub fn constant(&self, node: NodeId) -> Option<u64> {
        match self.expr(node) {
            Expr::Iconst { value, .. } => Some(*value),
            _ => None,
        }
    }

    fn best(&self, node: NodeId) -> NodeId {
        self.nodes[node.0].best
    }

    fn insert(&mut self, mut expr: Expr, rules: &[Box<dyn Rewrite>]) -> NodeId {
        if let Expr::Iconst { ty, value } = &mut expr {
            *value = ty.truncate(*value as i64);
        }
        if let Expr::Binary { args, .. } = &mut expr {
            *args = args.map(|arg| self.best(arg));
        }
        if let Some(&id) = self.interned.get(&expr) {
            return self.best(id);
        }
        let id = self.insert_raw(expr);
        let mut candidates = vec![id];
        let mut cursor = 0;
        while cursor < candidates.len() && cursor < 32 {
            let candidate = candidates[cursor];
            cursor += 1;
            for rule in rules {
                let expr = &self.nodes[candidate.0].expr;
                if !rule.accepts(expr) {
                    continue;
                }
                let Some(replacement) = rule.rewrite(self, expr) else {
                    continue;
                };
                assert_eq!(replacement.ty(), self.nodes[id.0].expr.ty());
                assert!(replacement.args().iter().all(|arg| arg.0 < id.0));
                let alternative = self.insert_raw(replacement);
                if !candidates.contains(&alternative) {
                    candidates.push(alternative);
                    self.nodes[id.0].alternatives.push(alternative);
                }
                let best = self.best(alternative);
                if self.nodes[best.0].cost < self.nodes[id.0].cost {
                    self.nodes[id.0].best = best;
                    self.nodes[id.0].cost = self.nodes[best.0].cost;
                }
            }
        }
        self.best(id)
    }

    fn insert_raw(&mut self, expr: Expr) -> NodeId {
        if let Some(&id) = self.interned.get(&expr) {
            return id;
        }
        let id = NodeId(self.nodes.len());
        let cost = expr.args().iter().fold(1u64, |cost, arg| {
            cost.saturating_add(self.nodes[arg.0].cost)
        });
        self.interned.insert(expr.clone(), id);
        self.nodes.push(Node {
            expr,
            alternatives: Vec::new(),
            best: id,
            cost,
        });
        id
    }
}

pub struct EGraphOptimizer {
    rules: Vec<Box<dyn Rewrite>>,
}

impl EGraphOptimizer {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add(&mut self, rule: impl Rewrite + 'static) -> &mut Self {
        self.rules.push(Box::new(rule));
        self.rules.sort_by_key(|rule| rule.priority());
        self
    }
}

impl Default for EGraphOptimizer {
    fn default() -> Self {
        let mut optimizer = Self::new();
        optimizer.add(ConstantFold);
        optimizer
    }
}

fn is_pure(data: &InstData) -> bool {
    matches!(
        data,
        InstData::Iconst { .. }
            | InstData::Binary {
                op: BinOp::Iadd
                    | BinOp::Isub
                    | BinOp::Imul
                    | BinOp::Band
                    | BinOp::Bor
                    | BinOp::Bxor,
                ..
            }
    )
}

impl Pass for EGraphOptimizer {
    fn name(&self) -> &'static str {
        "egraph"
    }

    fn run(&mut self, func: &mut Function) {
        let cfg = ControlFlow::compute(func);
        let mut pure = vec![false; func.num_insts()];
        for &block in cfg.reverse_postorder() {
            for &inst in func.block(block).insts() {
                pure[inst.index()] = is_pure(func.inst(inst));
            }
        }
        let mut graph = EGraph::default();
        let mut values = vec![None; func.num_values()];
        for (index, value_node) in values.iter_mut().enumerate() {
            let value = Value::from_index(index);
            if matches!(func.value_def(value), ValueDef::Param(..))
                || matches!(func.value_def(value), ValueDef::Result(inst) if !pure[inst.index()])
            {
                *value_node = Some(graph.insert(
                    Expr::Input {
                        ty: func.value_type(value),
                        value,
                    },
                    &[],
                ));
            }
        }
        for index in 0..values.len() {
            let mut stack = vec![(Value::from_index(index), false)];
            while let Some((value, ready)) = stack.pop() {
                if values[value.index()].is_some() {
                    continue;
                }
                let ValueDef::Result(inst) = func.value_def(value) else {
                    unreachable!();
                };
                if !ready {
                    stack.push((value, true));
                    stack.extend(func.inst(inst).args().iter().map(|&arg| (arg, false)));
                    continue;
                }
                let ty = func.value_type(value);
                let expr = match *func.inst(inst) {
                    InstData::Iconst { value, .. } => Expr::Iconst {
                        ty,
                        value: ty.truncate(value),
                    },
                    InstData::Binary { op, args } => Expr::Binary {
                        ty,
                        op,
                        args: args.map(|arg| values[arg.index()].unwrap()),
                    },
                    _ => unreachable!(),
                };
                values[value.index()] = Some(graph.insert(expr, &self.rules));
            }
        }

        let blocks: Vec<_> = cfg
            .reverse_postorder()
            .iter()
            .copied()
            .chain(func.blocks().filter(|&block| !cfg.is_reachable(block)))
            .collect();
        let mut emitted = HashMap::new();
        for block in blocks {
            let insts = func.take_block_insts(block);
            for inst in insts {
                if pure[inst.index()] {
                    continue;
                }
                let mut data = func.inst(inst).clone();
                data.map_args(|value| {
                    materialize(
                        values[value.index()].unwrap(),
                        block,
                        &graph,
                        &cfg,
                        &mut emitted,
                        func,
                    )
                });
                func.replace_inst(inst, data);
                func.append_existing_inst(block, inst);
            }
            if let Some(mut term) = func.block(block).terminator().cloned() {
                term.map_args(|value| {
                    materialize(
                        values[value.index()].unwrap(),
                        block,
                        &graph,
                        &cfg,
                        &mut emitted,
                        func,
                    )
                });
                func.set_terminator(block, term);
            }
        }
        func.compact();
    }
}

fn available(
    node: NodeId,
    block: Block,
    cfg: &ControlFlow,
    emitted: &HashMap<(Block, NodeId), Value>,
) -> Option<Value> {
    let mut scope = Some(block);
    while let Some(block) = scope {
        if let Some(&value) = emitted.get(&(block, node)) {
            return Some(value);
        }
        scope = cfg.idom(block);
    }
    None
}

fn materialize(
    root: NodeId,
    block: Block,
    graph: &EGraph,
    cfg: &ControlFlow,
    emitted: &mut HashMap<(Block, NodeId), Value>,
    func: &mut Function,
) -> Value {
    let root = graph.best(root);
    let mut stack = vec![(root, false)];
    while let Some((node, ready)) = stack.pop() {
        let node = graph.best(node);
        if available(node, block, cfg, emitted).is_some() {
            continue;
        }
        let expr = graph.expr(node);
        if let Expr::Input { value, .. } = expr {
            emitted.insert((block, node), *value);
            continue;
        }
        if !ready {
            stack.push((node, true));
            stack.extend(expr.args().iter().map(|&arg| (arg, false)));
            continue;
        }
        let data = match *expr {
            Expr::Iconst { ty, value } => InstData::Iconst {
                ty,
                value: value as i64,
            },
            Expr::Binary { op, args, .. } => InstData::Binary {
                op,
                args: args.map(|arg| available(graph.best(arg), block, cfg, emitted).unwrap()),
            },
            Expr::Input { .. } => unreachable!(),
        };
        let value = func.push_inst(block, data);
        emitted.insert((block, node), value);
    }
    available(root, block, cfg, emitted).unwrap()
}
