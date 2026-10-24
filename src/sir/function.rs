use super::{Block, Inst, InstData, Terminator, Type, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
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
    result: Value,
}

#[derive(Clone, Debug)]
pub struct Function {
    name: String,
    signature: Signature,
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

    pub fn push_inst(&mut self, block: Block, data: InstData) -> Value {
        let inst = Inst::from_index(self.insts.len());
        let result = self.new_value(data.result_type(), ValueDef::Result(inst));
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

    pub fn inst_result(&self, inst: Inst) -> Value {
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
