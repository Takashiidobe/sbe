use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection,
    IndirectNameMap, Instruction, Module, NameMap, NameSection, TypeSection, ValType,
};

use super::error::CompileError;
use crate::sir::{BinOp, Block, ControlFlow, Function, InstData, Terminator, Value, verify};

pub struct Compiled {
    pub code: Vec<u8>,
    pub wat: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    BlockFollowedBy(Block),
    LoopHeadedBy(Block),
    Other,
}

struct Emitter<'a> {
    func: &'a Function,
    cfg: ControlFlow,
    dom_children: Vec<Vec<Block>>,
    local_of: Vec<u32>,
    body: WasmFunction,
}

impl Emitter<'_> {
    fn op(&mut self, instruction: Instruction<'_>) {
        self.body.instruction(&instruction);
    }

    fn get(&mut self, value: Value) {
        self.op(Instruction::LocalGet(self.local_of[value.index()]));
    }

    fn emit_insts(&mut self, block: Block) {
        for &inst in self.func.block(block).insts() {
            let result = self.local_of[self.func.inst_result(inst).index()];
            match self.func.inst(inst) {
                InstData::Iconst { value } => self.op(Instruction::I64Const(*value)),
                InstData::Binary {
                    op: BinOp::Iadd,
                    args: [a, b],
                } => {
                    self.get(*a);
                    self.get(*b);
                    self.op(Instruction::I64Add);
                }
            }
            self.op(Instruction::LocalSet(result));
        }
    }

    fn emit_return(&mut self, values: &[Value]) {
        for &value in values {
            self.get(value);
        }
        self.op(Instruction::Return);
    }

    /// pushes every arg before setting any param, so the assignment is simultaneous
    fn emit_moves(&mut self, dest: Block, args: &[Value]) {
        for &arg in args {
            self.get(arg);
        }
        for &param in self.func.block(dest).params().iter().rev() {
            self.op(Instruction::LocalSet(self.local_of[param.index()]));
        }
    }

    fn emit_condition(&mut self, cond: Value) {
        self.get(cond);
        self.op(Instruction::I64Const(0));
        self.op(Instruction::I64Ne);
    }

    fn is_merge(&self, block: Block) -> bool {
        let forward = |&&p: &&Block| !self.cfg.is_retreating(p, block);
        self.cfg.predecessors(block).iter().filter(forward).count() >= 2
    }

    fn is_loop_header(&self, block: Block) -> bool {
        self.cfg
            .predecessors(block)
            .iter()
            .any(|&p| self.cfg.is_retreating(p, block))
    }

    // structured control flow from the dominator tree, after ramsey's "beyond relooper"
    fn do_tree(&mut self, x: Block, ctx: &mut Vec<Frame>) {
        let merges: Vec<Block> = self.dom_children[x.index()]
            .iter()
            .copied()
            .filter(|&y| self.is_merge(y))
            .collect();
        if self.is_loop_header(x) {
            self.op(Instruction::Loop(BlockType::Empty));
            ctx.push(Frame::LoopHeadedBy(x));
            self.node_within(x, &merges, ctx);
            ctx.pop();
            self.op(Instruction::End);
        } else {
            self.node_within(x, &merges, ctx);
        }
    }

    fn node_within(&mut self, x: Block, merges: &[Block], ctx: &mut Vec<Frame>) {
        if let Some((&y, rest)) = merges.split_last() {
            self.op(Instruction::Block(BlockType::Empty));
            ctx.push(Frame::BlockFollowedBy(y));
            self.node_within(x, rest, ctx);
            ctx.pop();
            self.op(Instruction::End);
            self.do_tree(y, ctx);
            return;
        }

        self.emit_insts(x);
        match self.func.block(x).terminator().expect("verified function") {
            Terminator::Return { values } => self.emit_return(values),
            Terminator::Jump { dest, args } => self.do_branch(x, *dest, args, ctx),
            Terminator::Brif {
                cond,
                then_dest,
                then_args,
                else_dest,
                else_args,
            } => {
                self.emit_condition(*cond);
                self.op(Instruction::If(BlockType::Empty));
                ctx.push(Frame::Other);
                self.do_branch(x, *then_dest, then_args, ctx);
                self.op(Instruction::Else);
                self.do_branch(x, *else_dest, else_args, ctx);
                ctx.pop();
                self.op(Instruction::End);
            }
        }
    }

    fn do_branch(&mut self, from: Block, to: Block, args: &[Value], ctx: &mut Vec<Frame>) {
        self.emit_moves(to, args);
        let retreating = self.cfg.is_retreating(from, to);
        if !retreating && !self.is_merge(to) {
            self.do_tree(to, ctx);
            return;
        }
        let wanted = if retreating {
            Frame::LoopHeadedBy(to)
        } else {
            Frame::BlockFollowedBy(to)
        };
        let depth = ctx
            .iter()
            .rev()
            .position(|&f| f == wanted)
            .expect("branch target is in scope");
        self.op(Instruction::Br(depth as u32));
    }

    // fallback for irreducible control flow: a loop around a `br_table` on a block index local
    fn dispatch(&mut self, pc: u32) {
        let order = self.cfg.reverse_postorder().to_vec();
        let n = order.len() as u32;
        let index_of = |block: Block| order.iter().position(|&b| b == block).unwrap() as i32;

        self.op(Instruction::Loop(BlockType::Empty));
        for _ in 0..n {
            self.op(Instruction::Block(BlockType::Empty));
        }
        self.op(Instruction::LocalGet(pc));
        let targets: Vec<u32> = (0..n).collect();
        self.op(Instruction::BrTable(targets.into(), n - 1));

        for (k, &block) in order.iter().enumerate() {
            self.op(Instruction::End);
            self.emit_insts(block);
            let loop_depth = n - 1 - k as u32;
            let terminator = self.func.block(block).terminator().expect("verified");
            match terminator {
                Terminator::Return { values } => self.emit_return(values),
                Terminator::Jump { dest, args } => {
                    self.dispatch_edge(*dest, args, index_of(*dest), pc, loop_depth);
                }
                Terminator::Brif {
                    cond,
                    then_dest,
                    then_args,
                    else_dest,
                    else_args,
                } => {
                    self.emit_condition(*cond);
                    self.op(Instruction::If(BlockType::Empty));
                    self.dispatch_edge(
                        *then_dest,
                        then_args,
                        index_of(*then_dest),
                        pc,
                        loop_depth + 1,
                    );
                    self.op(Instruction::Else);
                    self.dispatch_edge(
                        *else_dest,
                        else_args,
                        index_of(*else_dest),
                        pc,
                        loop_depth + 1,
                    );
                    self.op(Instruction::End);
                }
            }
        }
        self.op(Instruction::End);
    }

    fn dispatch_edge(&mut self, dest: Block, args: &[Value], index: i32, pc: u32, depth: u32) {
        self.emit_moves(dest, args);
        self.op(Instruction::I32Const(index));
        self.op(Instruction::LocalSet(pc));
        self.op(Instruction::Br(depth));
    }
}

/// every sir value lives in its own wasm local; params are the first locals
pub fn compile(func: &Function) -> Result<Compiled, CompileError> {
    verify(func)?;

    let cfg = ControlFlow::compute(func);
    let mut local_values: Vec<Value> = Vec::new();
    let mut local_of = vec![0u32; func.num_values()];
    let mut define = |value: Value| {
        local_of[value.index()] = local_values.len() as u32;
        local_values.push(value);
    };
    let num_params = func.block(func.entry_block()).params().len();
    for &block in cfg.reverse_postorder() {
        let data = func.block(block);
        data.params().iter().for_each(|&p| define(p));
        data.insts()
            .iter()
            .for_each(|&inst| define(func.inst_result(inst)));
    }

    let mut dom_children = vec![Vec::new(); func.blocks().count()];
    for &block in cfg.reverse_postorder() {
        if let Some(parent) = cfg.idom(block) {
            dom_children[parent.index()].push(block);
        }
    }

    let reducible = cfg.is_reducible();
    let mut locals = vec![((local_values.len() - num_params) as u32, ValType::I64)];
    if !reducible {
        locals.push((1, ValType::I32));
    }
    let mut emitter = Emitter {
        func,
        cfg,
        dom_children,
        local_of,
        body: WasmFunction::new(locals),
    };
    if reducible {
        emitter.do_tree(func.entry_block(), &mut Vec::new());
    } else {
        emitter.dispatch(local_values.len() as u32);
    }
    // the structured tail is statically reachable even though every path returns or branches
    emitter.op(Instruction::Unreachable);
    emitter.op(Instruction::End);
    let body = emitter.body;

    let signature = func.signature();
    let mut types = TypeSection::new();
    types.ty().function(
        signature.params.iter().map(|_| ValType::I64),
        signature.returns.iter().map(|_| ValType::I64),
    );
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut exports = ExportSection::new();
    exports.export(func.name(), ExportKind::Func, 0);
    let mut code = CodeSection::new();
    code.function(&body);

    let mut function_names = NameMap::new();
    function_names.append(0, func.name());
    let mut local_names = NameMap::new();
    for (index, value) in local_values.iter().enumerate() {
        local_names.append(index as u32, &value.to_string());
    }
    let mut all_local_names = IndirectNameMap::new();
    all_local_names.append(0, &local_names);
    let mut names = NameSection::new();
    names.functions(&function_names);
    names.locals(&all_local_names);

    let mut module = Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code)
        .section(&names);
    let bytes = module.finish();

    let wat = wasmprinter::print_bytes(&bytes).map_err(|e| CompileError::Print(e.to_string()))?;
    Ok(Compiled { code: bytes, wat })
}
