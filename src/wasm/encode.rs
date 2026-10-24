use std::collections::HashMap;

use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function as WasmFunction,
    FunctionSection, ImportSection, IndirectNameMap, Instruction, Module as WasmModule, NameMap,
    NameSection, TypeSection, ValType,
};

use super::error::CompileError;
use crate::sir::{
    BinOp, Block, ControlFlow, CvtOp, Function, InstData, IntCC, Module, Signature, Terminator,
    Type, UnOp, Value, ValueDef, verify_module,
};

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
    callee_index: Vec<u32>,
    body: WasmFunction,
}

impl Emitter<'_> {
    fn op(&mut self, instruction: Instruction<'_>) {
        self.body.instruction(&instruction);
    }

    fn get(&mut self, value: Value) {
        self.op(Instruction::LocalGet(self.local_of[value.index()]));
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(value)
    }

    /// i8 and i16 live in i32 locals with undefined upper bits, so the bits that matter are
    /// extended before an op that can see them
    fn get_extended(&mut self, value: Value, signed: bool) {
        self.get(value);
        match (self.ty(value), signed) {
            (Type::I8, true) => self.op(Instruction::I32Extend8S),
            (Type::I16, true) => self.op(Instruction::I32Extend16S),
            (ty @ (Type::I8 | Type::I16), false) => {
                self.op(Instruction::I32Const((1 << ty.bits()) - 1));
                self.op(Instruction::I32And);
            }
            _ => {}
        }
    }

    // an icmp result is exactly 0 or 1; any other i8 may carry garbage above bit 7
    fn get_cond(&mut self, value: Value) {
        self.get(value);
        let from_icmp = match self.func.value_def(value) {
            ValueDef::Result(inst) => matches!(self.func.inst(inst), InstData::Icmp { .. }),
            ValueDef::Param(..) => false,
        };
        if !from_icmp {
            self.op(Instruction::I32Const(0xff));
            self.op(Instruction::I32And);
        }
    }

    // wasm's `i64.rem_s` yields 0 for `i64::MIN % -1`, but the ir (like x86 `idiv`) traps
    fn trap_on_signed_overflow(&mut self, a: Value, b: Value) {
        self.get(b);
        self.op(Instruction::I64Const(-1));
        self.op(Instruction::I64Eq);
        self.get(a);
        self.op(Instruction::I64Const(i64::MIN));
        self.op(Instruction::I64Eq);
        self.op(Instruction::I32And);
        self.op(Instruction::If(BlockType::Empty));
        self.op(Instruction::Unreachable);
        self.op(Instruction::End);
    }

    fn emit_binary(&mut self, op: BinOp, ty: Type, a: Value, b: Value) {
        let signed = matches!(op, BinOp::Sshr | BinOp::Sdiv | BinOp::Srem);
        let shift = matches!(op, BinOp::Ishl | BinOp::Ushr | BinOp::Sshr);
        // i32 division is done in i64 so that the overflow case wraps like i8 and i16 do
        let widen = ty == Type::I32 && signed && !shift;
        let wide = ty == Type::I64;

        if wide && op == BinOp::Srem {
            self.trap_on_signed_overflow(a, b);
        }
        let extended = !matches!(op, BinOp::Iadd | BinOp::Isub | BinOp::Imul | BinOp::Band)
            && !matches!(op, BinOp::Bor | BinOp::Bxor | BinOp::Ishl);
        for (i, value) in [a, b].into_iter().enumerate() {
            if extended && !(shift && i == 1) {
                self.get_extended(value, signed);
            } else {
                self.get(value);
            }
            if widen {
                self.op(Instruction::I64ExtendI32S);
            }
            if shift && i == 1 && matches!(ty, Type::I8 | Type::I16) {
                self.op(Instruction::I32Const(ty.bits() as i32 - 1));
                self.op(Instruction::I32And);
            }
        }
        self.op(match (wide || widen, op) {
            (true, BinOp::Iadd) => Instruction::I64Add,
            (true, BinOp::Isub) => Instruction::I64Sub,
            (true, BinOp::Imul) => Instruction::I64Mul,
            (true, BinOp::Band) => Instruction::I64And,
            (true, BinOp::Bor) => Instruction::I64Or,
            (true, BinOp::Bxor) => Instruction::I64Xor,
            (true, BinOp::Ishl) => Instruction::I64Shl,
            (true, BinOp::Ushr) => Instruction::I64ShrU,
            (true, BinOp::Sshr) => Instruction::I64ShrS,
            (true, BinOp::Sdiv) => Instruction::I64DivS,
            (true, BinOp::Udiv) => Instruction::I64DivU,
            (true, BinOp::Srem) => Instruction::I64RemS,
            (true, BinOp::Urem) => Instruction::I64RemU,
            (false, BinOp::Iadd) => Instruction::I32Add,
            (false, BinOp::Isub) => Instruction::I32Sub,
            (false, BinOp::Imul) => Instruction::I32Mul,
            (false, BinOp::Band) => Instruction::I32And,
            (false, BinOp::Bor) => Instruction::I32Or,
            (false, BinOp::Bxor) => Instruction::I32Xor,
            (false, BinOp::Ishl) => Instruction::I32Shl,
            (false, BinOp::Ushr) => Instruction::I32ShrU,
            (false, BinOp::Sshr) => Instruction::I32ShrS,
            (false, BinOp::Sdiv) => Instruction::I32DivS,
            (false, BinOp::Udiv) => Instruction::I32DivU,
            (false, BinOp::Srem) => Instruction::I32RemS,
            (false, BinOp::Urem) => Instruction::I32RemU,
        });
        if widen {
            self.op(Instruction::I32WrapI64);
        }
    }

    fn emit_icmp(&mut self, cc: IntCC, a: Value, b: Value) {
        let signed = matches!(cc, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
        self.get_extended(a, signed);
        self.get_extended(b, signed);
        let wide = self.ty(a) == Type::I64;
        self.op(match (wide, cc) {
            (true, IntCC::Eq) => Instruction::I64Eq,
            (true, IntCC::Ne) => Instruction::I64Ne,
            (true, IntCC::Slt) => Instruction::I64LtS,
            (true, IntCC::Sle) => Instruction::I64LeS,
            (true, IntCC::Sgt) => Instruction::I64GtS,
            (true, IntCC::Sge) => Instruction::I64GeS,
            (true, IntCC::Ult) => Instruction::I64LtU,
            (true, IntCC::Ule) => Instruction::I64LeU,
            (true, IntCC::Ugt) => Instruction::I64GtU,
            (true, IntCC::Uge) => Instruction::I64GeU,
            (false, IntCC::Eq) => Instruction::I32Eq,
            (false, IntCC::Ne) => Instruction::I32Ne,
            (false, IntCC::Slt) => Instruction::I32LtS,
            (false, IntCC::Sle) => Instruction::I32LeS,
            (false, IntCC::Sgt) => Instruction::I32GtS,
            (false, IntCC::Sge) => Instruction::I32GeS,
            (false, IntCC::Ult) => Instruction::I32LtU,
            (false, IntCC::Ule) => Instruction::I32LeU,
            (false, IntCC::Ugt) => Instruction::I32GtU,
            (false, IntCC::Uge) => Instruction::I32GeU,
        });
    }

    fn emit_convert(&mut self, op: CvtOp, ty: Type, arg: Value) {
        let to_i64 = ty == Type::I64;
        match op {
            CvtOp::Sextend | CvtOp::Uextend => {
                let signed = op == CvtOp::Sextend;
                self.get_extended(arg, signed);
                if to_i64 && signed {
                    self.op(Instruction::I64ExtendI32S);
                } else if to_i64 {
                    self.op(Instruction::I64ExtendI32U);
                }
            }
            CvtOp::Ireduce => {
                self.get(arg);
                if self.ty(arg) == Type::I64 {
                    self.op(Instruction::I32WrapI64);
                }
            }
        }
    }

    fn emit_insts(&mut self, block: Block) {
        for &inst in self.func.block(block).insts() {
            let result = self
                .func
                .inst_result(inst)
                .map(|v| self.local_of[v.index()]);
            match self.func.inst(inst) {
                InstData::Iconst {
                    ty: Type::I64,
                    value,
                } => self.op(Instruction::I64Const(*value)),
                InstData::Iconst { value, .. } => self.op(Instruction::I32Const(*value as i32)),
                InstData::Convert { op, ty, arg } => self.emit_convert(*op, *ty, *arg),
                InstData::Icmp { cc, args: [a, b] } => self.emit_icmp(*cc, *a, *b),
                InstData::Select {
                    args: [cond, then_value, else_value],
                } => {
                    self.get(*then_value);
                    self.get(*else_value);
                    self.get_cond(*cond);
                    self.op(Instruction::Select);
                }
                InstData::Unary { op, arg } => {
                    let wide = self.ty(*arg) == Type::I64;
                    match (op, wide) {
                        (UnOp::Ineg, true) => {
                            self.op(Instruction::I64Const(0));
                            self.get(*arg);
                            self.op(Instruction::I64Sub);
                        }
                        (UnOp::Ineg, false) => {
                            self.op(Instruction::I32Const(0));
                            self.get(*arg);
                            self.op(Instruction::I32Sub);
                        }
                        (UnOp::Bnot, true) => {
                            self.get(*arg);
                            self.op(Instruction::I64Const(-1));
                            self.op(Instruction::I64Xor);
                        }
                        (UnOp::Bnot, false) => {
                            self.get(*arg);
                            self.op(Instruction::I32Const(-1));
                            self.op(Instruction::I32Xor);
                        }
                    }
                }
                InstData::Binary { op, args: [a, b] } => {
                    self.emit_binary(*op, self.ty(*a), *a, *b);
                }
                InstData::Call { callee, args } => {
                    for &arg in args {
                        self.get(arg);
                    }
                    self.op(Instruction::Call(self.callee_index[callee.index()]));
                }
            }
            if let Some(result) = result {
                self.op(Instruction::LocalSet(result));
            }
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
                self.get_cond(*cond);
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
                    self.get_cond(*cond);
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
fn compile_body(
    func: &Function,
    function_index: &HashMap<&str, u32>,
) -> (WasmFunction, Vec<Value>) {
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
            .filter_map(|&inst| func.inst_result(inst))
            .for_each(&mut define);
    }

    let mut dom_children = vec![Vec::new(); func.blocks().count()];
    for &block in cfg.reverse_postorder() {
        if let Some(parent) = cfg.idom(block) {
            dom_children[parent.index()].push(block);
        }
    }

    let reducible = cfg.is_reducible();
    let mut locals: Vec<(u32, ValType)> = Vec::new();
    for &value in &local_values[num_params..] {
        let ty = val_type(func.value_type(value));
        match locals.last_mut() {
            Some((count, last)) if *last == ty => *count += 1,
            _ => locals.push((1, ty)),
        }
    }
    if !reducible {
        locals.push((1, ValType::I32));
    }
    let mut emitter = Emitter {
        func,
        cfg,
        dom_children,
        local_of,
        callee_index: func
            .callees()
            .iter()
            .map(|callee| function_index[callee.name.as_str()])
            .collect(),
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

    (body, local_values)
}

/// a function that calls others gets each distinct callee as an import from `env`
pub fn compile(func: &Function) -> Result<Compiled, CompileError> {
    let mut module = Module::new();
    for callee in func.callees() {
        let known = module.imports().iter().any(|i| i.name == callee.name);
        if callee.name != func.name() && !known {
            module.declare_import(callee.name.clone(), callee.signature.clone());
        }
    }
    module.add_function(func.clone());
    compile_module(&module)
}

/// imports come first in the function index space, in declaration order, under module `env`;
/// every defined function is exported by name
pub fn compile_module(module: &Module) -> Result<Compiled, CompileError> {
    verify_module(module)?;

    let mut function_index: HashMap<&str, u32> = HashMap::new();
    let imports = module.imports().iter().map(|i| i.name.as_str());
    let defined = module.functions().iter().map(Function::name);
    for (index, name) in imports.chain(defined).enumerate() {
        function_index.insert(name, index as u32);
    }

    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    let mut function_names = NameMap::new();
    let mut all_local_names = IndirectNameMap::new();

    let mut next_type = 0;
    let mut add_type = |signature: &Signature| {
        types.ty().function(
            signature.params.iter().map(|&ty| val_type(ty)),
            signature.returns.iter().map(|&ty| val_type(ty)),
        );
        next_type += 1;
        next_type - 1
    };

    for import in module.imports() {
        let ty = add_type(&import.signature);
        imports.import("env", &import.name, EntityType::Function(ty));
        function_names.append(function_index[import.name.as_str()], &import.name);
    }
    for func in module.functions() {
        let index = function_index[func.name()];
        functions.function(add_type(func.signature()));
        exports.export(func.name(), ExportKind::Func, index);
        function_names.append(index, func.name());

        let (body, local_values) = compile_body(func, &function_index);
        code.function(&body);
        let mut local_names = NameMap::new();
        for (local, value) in local_values.iter().enumerate() {
            local_names.append(local as u32, &value.to_string());
        }
        all_local_names.append(index, &local_names);
    }

    let mut names = NameSection::new();
    names.functions(&function_names);
    names.locals(&all_local_names);

    let mut wasm = WasmModule::new();
    wasm.section(&types);
    if !module.imports().is_empty() {
        wasm.section(&imports);
    }
    wasm.section(&functions)
        .section(&exports)
        .section(&code)
        .section(&names);
    let bytes = wasm.finish();

    let wat = wasmprinter::print_bytes(&bytes).map_err(|e| CompileError::Print(e.to_string()))?;
    Ok(Compiled { code: bytes, wat })
}

fn val_type(ty: Type) -> ValType {
    match ty {
        Type::I8 | Type::I16 | Type::I32 => ValType::I32,
        Type::I64 => ValType::I64,
    }
}
