use super::{Block, FuncRef, Inst, InstData, StackSlot, Terminator, Type, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
}

/// a function this function calls, declared by name and signature only
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Callee {
    pub name: String,
    pub signature: Signature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueDef {
    Result(Inst),
    Param(Block, usize),
}

#[derive(Clone, Debug, Default)]
pub struct BlockData {
    params: Vec<Value>,
    insts: Vec<Inst>,
    terminator: Option<Terminator>,
}

impl BlockData {
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    pub fn insts(&self) -> &[Inst] {
        &self.insts
    }

    pub fn terminator(&self) -> Option<&Terminator> {
        self.terminator.as_ref()
    }
}

#[derive(Clone, Debug)]
struct ValueEntry {
    ty: Type,
    def: ValueDef,
}

#[derive(Clone, Debug)]
struct InstEntry {
    data: InstData,
    result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Function {
    name: String,
    signature: Signature,
    callees: Vec<Callee>,
    stack_slots: Vec<u32>,
    blocks: Vec<BlockData>,
    insts: Vec<InstEntry>,
    values: Vec<ValueEntry>,
}

impl Function {
    /// creates the entry block, whose params are the function params
    pub fn new(name: impl Into<String>, signature: Signature) -> Self {
        let mut func = Self {
            name: name.into(),
            signature,
            callees: Vec::new(),
            stack_slots: Vec::new(),
            blocks: Vec::new(),
            insts: Vec::new(),
            values: Vec::new(),
        };
        let entry = func.create_block();
        for ty in func.signature.params.clone() {
            func.append_block_param(entry, ty);
        }
        func
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn entry_block(&self) -> Block {
        Block::from_index(0)
    }

    pub fn create_block(&mut self) -> Block {
        self.blocks.push(BlockData::default());
        Block::from_index(self.blocks.len() - 1)
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Value {
        let index = self.blocks[block.index()].params.len();
        let value = self.new_value(ty, ValueDef::Param(block, index));
        self.blocks[block.index()].params.push(value);
        value
    }

    pub fn declare_callee(&mut self, name: impl Into<String>, signature: Signature) -> FuncRef {
        self.callees.push(Callee {
            name: name.into(),
            signature,
        });
        FuncRef::from_index(self.callees.len() - 1)
    }

    /// `size` is in bytes; every slot is 8-byte aligned
    pub fn create_stack_slot(&mut self, size: u32) -> StackSlot {
        self.stack_slots.push(size);
        StackSlot::from_index(self.stack_slots.len() - 1)
    }

    /// appends an instruction that yields a value; calls and stores go through
    /// [`Function::push_call`] and [`Function::push_store`]
    pub fn push_inst(&mut self, block: Block, data: InstData) -> Value {
        assert!(
            !matches!(data, InstData::Call { .. } | InstData::Store { .. }),
            "calls and stores may not yield a value"
        );
        self.insert_inst(block, data)
            .expect("only calls and stores are void")
    }

    pub fn push_store(&mut self, block: Block, value: Value, addr: Value) {
        self.insert_inst(
            block,
            InstData::Store {
                args: [value, addr],
            },
        );
    }

    pub fn push_call(&mut self, block: Block, callee: FuncRef, args: Vec<Value>) -> Option<Value> {
        self.insert_inst(block, InstData::Call { callee, args })
    }

    fn insert_inst(&mut self, block: Block, data: InstData) -> Option<Value> {
        let inst = Inst::from_index(self.insts.len());
        // a forward reference has no type yet; the verifier reports it, so any guess will do
        let ty = data.result_type(
            |v| self.values.get(v.index()).map_or(Type::I64, |e| e.ty),
            |f| {
                self.callees
                    .get(f.index())
                    .and_then(|c| c.signature.returns.first().copied())
            },
        );
        let result = ty.map(|ty| self.new_value(ty, ValueDef::Result(inst)));
        self.insts.push(InstEntry { data, result });
        self.blocks[block.index()].insts.push(inst);
        result
    }

    pub fn set_terminator(&mut self, block: Block, terminator: Terminator) {
        self.blocks[block.index()].terminator = Some(terminator);
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block> + use<> {
        (0..self.blocks.len()).map(Block::from_index)
    }

    pub fn block(&self, block: Block) -> &BlockData {
        &self.blocks[block.index()]
    }

    pub fn inst(&self, inst: Inst) -> &InstData {
        &self.insts[inst.index()].data
    }

    pub fn replace_inst(&mut self, inst: Inst, data: InstData) {
        let ty = data.result_type(
            |v| self.value_type(v),
            |f| self.callee(f).signature.returns.first().copied(),
        );
        assert_eq!(ty, self.inst_result(inst).map(|v| self.value_type(v)));
        self.insts[inst.index()].data = data;
    }

    pub(crate) fn take_block_insts(&mut self, block: Block) -> Vec<Inst> {
        std::mem::take(&mut self.blocks[block.index()].insts)
    }

    pub(crate) fn append_existing_inst(&mut self, block: Block, inst: Inst) {
        self.blocks[block.index()].insts.push(inst);
    }

    pub(crate) fn compact(&mut self) {
        let mut value_map = vec![None; self.values.len()];
        let mut inst_map = vec![None; self.insts.len()];
        let mut old_values = Vec::new();
        let mut old_insts = Vec::new();
        for block in &self.blocks {
            for &value in &block.params {
                value_map[value.index()] = Some(Value::from_index(old_values.len()));
                old_values.push(value);
            }
            for &inst in &block.insts {
                inst_map[inst.index()] = Some(Inst::from_index(old_insts.len()));
                old_insts.push(inst);
                if let Some(value) = self.inst_result(inst) {
                    value_map[value.index()] = Some(Value::from_index(old_values.len()));
                    old_values.push(value);
                }
            }
        }
        let map_value = |value: Value| value_map[value.index()].expect("value is in the layout");
        let values = old_values
            .into_iter()
            .map(|value| {
                let mut entry = self.values[value.index()].clone();
                if let ValueDef::Result(inst) = &mut entry.def {
                    *inst = inst_map[inst.index()].expect("definition is in the layout");
                }
                entry
            })
            .collect();
        let insts = old_insts
            .into_iter()
            .map(|inst| {
                let mut entry = self.insts[inst.index()].clone();
                entry.result = entry.result.map(map_value);
                entry.data.map_args(map_value);
                entry
            })
            .collect();
        for block in &mut self.blocks {
            for value in &mut block.params {
                *value = map_value(*value);
            }
            for inst in &mut block.insts {
                *inst = inst_map[inst.index()].unwrap();
            }
            if let Some(term) = &mut block.terminator {
                term.map_args(map_value);
            }
        }
        self.values = values;
        self.insts = insts;
    }

    pub fn inst_result(&self, inst: Inst) -> Option<Value> {
        self.insts[inst.index()].result
    }

    pub fn value_type(&self, value: Value) -> Type {
        self.values[value.index()].ty
    }

    pub fn value_def(&self, value: Value) -> ValueDef {
        self.values[value.index()].def
    }

    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    pub fn num_insts(&self) -> usize {
        self.insts.len()
    }

    pub fn callees(&self) -> &[Callee] {
        &self.callees
    }

    pub fn callee(&self, callee: FuncRef) -> &Callee {
        &self.callees[callee.index()]
    }

    pub fn stack_slots(&self) -> &[u32] {
        &self.stack_slots
    }

    pub fn contains_stack_slot(&self, slot: StackSlot) -> bool {
        slot.index() < self.stack_slots.len()
    }

    pub fn contains_callee(&self, callee: FuncRef) -> bool {
        callee.index() < self.callees.len()
    }

    pub fn contains_block(&self, block: Block) -> bool {
        block.index() < self.blocks.len()
    }

    pub fn contains_value(&self, value: Value) -> bool {
        value.index() < self.values.len()
    }

    fn new_value(&mut self, ty: Type, def: ValueDef) -> Value {
        self.values.push(ValueEntry { ty, def });
        Value::from_index(self.values.len() - 1)
    }
}
