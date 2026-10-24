use miette::Diagnostic;
use thiserror::Error;

use super::{
    BinOp, Block, CvtOp, FuncRef, Function, InstData, IntCC, Signature, StackSlot, Terminator,
    Type, UnOp, Value, VerifyErrors, verify::verify,
};

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
pub enum BuilderError {
    #[error("no current block; call switch_to_block first")]
    #[diagnostic(code(sir::builder::no_current_block))]
    NoCurrentBlock,

    #[error("{0} does not exist in this function")]
    #[diagnostic(code(sir::builder::unknown_block))]
    UnknownBlock(Block),

    #[error("{0} is not defined in this function")]
    #[diagnostic(
        code(sir::builder::unknown_value),
        help("values come from instruction results or block params of this builder")
    )]
    UnknownValue(Value),

    #[error("{0} is not declared in this function")]
    #[diagnostic(code(sir::builder::unknown_callee))]
    UnknownCallee(FuncRef),

    #[error("{0} is not a stack slot of this function")]
    #[diagnostic(code(sir::builder::unknown_stack_slot))]
    UnknownStackSlot(StackSlot),

    #[error("{0} is already terminated")]
    #[diagnostic(code(sir::builder::block_terminated))]
    BlockTerminated(Block),
}

pub struct FunctionBuilder {
    func: Function,
    current: Option<Block>,
}

impl FunctionBuilder {
    pub fn new(name: impl Into<String>, signature: Signature) -> Self {
        Self {
            func: Function::new(name, signature),
            current: None,
        }
    }

    pub fn entry_block(&self) -> Block {
        self.func.entry_block()
    }

    pub fn declare_callee(&mut self, name: impl Into<String>, signature: Signature) -> FuncRef {
        self.func.declare_callee(name, signature)
    }

    pub fn create_stack_slot(&mut self, size: u32) -> StackSlot {
        self.func.create_stack_slot(size)
    }

    pub fn create_block(&mut self) -> Block {
        self.func.create_block()
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Result<Value, BuilderError> {
        self.check_block(block)?;
        Ok(self.func.append_block_param(block, ty))
    }

    pub fn block_params(&self, block: Block) -> Result<&[Value], BuilderError> {
        self.check_block(block)?;
        Ok(self.func.block(block).params())
    }

    pub fn switch_to_block(&mut self, block: Block) -> Result<(), BuilderError> {
        self.check_block(block)?;
        self.current = Some(block);
        Ok(())
    }

    pub fn iconst(&mut self, value: i64) -> Result<Value, BuilderError> {
        self.iconst_typed(Type::I64, value)
    }

    pub fn iconst_typed(&mut self, ty: Type, value: i64) -> Result<Value, BuilderError> {
        self.emit(InstData::Iconst { ty, value })
    }

    pub fn convert(&mut self, op: CvtOp, ty: Type, arg: Value) -> Result<Value, BuilderError> {
        self.check_value(arg)?;
        self.emit(InstData::Convert { op, ty, arg })
    }

    pub fn unary(&mut self, op: UnOp, arg: Value) -> Result<Value, BuilderError> {
        self.check_value(arg)?;
        self.emit(InstData::Unary { op, arg })
    }

    pub fn icmp(&mut self, cc: IntCC, a: Value, b: Value) -> Result<Value, BuilderError> {
        self.check_value(a)?;
        self.check_value(b)?;
        self.emit(InstData::Icmp { cc, args: [a, b] })
    }

    pub fn select(
        &mut self,
        cond: Value,
        then_value: Value,
        else_value: Value,
    ) -> Result<Value, BuilderError> {
        self.check_value(cond)?;
        self.check_value(then_value)?;
        self.check_value(else_value)?;
        self.emit(InstData::Select {
            args: [cond, then_value, else_value],
        })
    }

    pub fn binary(&mut self, op: BinOp, a: Value, b: Value) -> Result<Value, BuilderError> {
        self.check_value(a)?;
        self.check_value(b)?;
        self.emit(InstData::Binary { op, args: [a, b] })
    }

    /// the result is `None` when the callee returns nothing
    pub fn call(&mut self, callee: FuncRef, args: &[Value]) -> Result<Option<Value>, BuilderError> {
        if !self.func.contains_callee(callee) {
            return Err(BuilderError::UnknownCallee(callee));
        }
        args.iter().try_for_each(|&v| self.check_value(v))?;
        let block = self.open_block()?;
        Ok(self.func.push_call(block, callee, args.to_vec()))
    }

    pub fn load(&mut self, ty: Type, addr: Value) -> Result<Value, BuilderError> {
        self.check_value(addr)?;
        self.emit(InstData::Load { ty, addr })
    }

    pub fn store(&mut self, value: Value, addr: Value) -> Result<(), BuilderError> {
        self.check_value(value)?;
        self.check_value(addr)?;
        let block = self.open_block()?;
        self.func.push_store(block, value, addr);
        Ok(())
    }

    pub fn stack_addr(&mut self, slot: StackSlot) -> Result<Value, BuilderError> {
        if !self.func.contains_stack_slot(slot) {
            return Err(BuilderError::UnknownStackSlot(slot));
        }
        self.emit(InstData::StackAddr { slot })
    }

    pub fn iadd(&mut self, a: Value, b: Value) -> Result<Value, BuilderError> {
        self.binary(BinOp::Iadd, a, b)
    }

    pub fn ret(&mut self, value: Value) -> Result<(), BuilderError> {
        self.ret_values(&[value])
    }

    pub fn ret_values(&mut self, values: &[Value]) -> Result<(), BuilderError> {
        values.iter().try_for_each(|&v| self.check_value(v))?;
        self.terminate(Terminator::Return {
            values: values.to_vec(),
        })
    }

    pub fn jump(&mut self, dest: Block, args: &[Value]) -> Result<(), BuilderError> {
        self.check_block(dest)?;
        args.iter().try_for_each(|&v| self.check_value(v))?;
        self.terminate(Terminator::Jump {
            dest,
            args: args.to_vec(),
        })
    }

    pub fn brif(
        &mut self,
        cond: Value,
        then_dest: Block,
        then_args: &[Value],
        else_dest: Block,
        else_args: &[Value],
    ) -> Result<(), BuilderError> {
        self.check_value(cond)?;
        self.check_block(then_dest)?;
        self.check_block(else_dest)?;
        then_args
            .iter()
            .chain(else_args)
            .try_for_each(|&v| self.check_value(v))?;
        self.terminate(Terminator::Brif {
            cond,
            then_dest,
            then_args: then_args.to_vec(),
            else_dest,
            else_args: else_args.to_vec(),
        })
    }

    pub fn finish(self) -> Result<Function, VerifyErrors> {
        verify(&self.func)?;
        Ok(self.func)
    }

    fn emit(&mut self, data: InstData) -> Result<Value, BuilderError> {
        let block = self.open_block()?;
        Ok(self.func.push_inst(block, data))
    }

    fn terminate(&mut self, terminator: Terminator) -> Result<(), BuilderError> {
        let block = self.open_block()?;
        self.func.set_terminator(block, terminator);
        Ok(())
    }

    fn open_block(&self) -> Result<Block, BuilderError> {
        let block = self.current.ok_or(BuilderError::NoCurrentBlock)?;
        if self.func.block(block).terminator().is_some() {
            return Err(BuilderError::BlockTerminated(block));
        }
        Ok(block)
    }

    fn check_block(&self, block: Block) -> Result<(), BuilderError> {
        self.func
            .contains_block(block)
            .then_some(())
            .ok_or(BuilderError::UnknownBlock(block))
    }

    fn check_value(&self, value: Value) -> Result<(), BuilderError> {
        self.func
            .contains_value(value)
            .then_some(())
            .ok_or(BuilderError::UnknownValue(value))
    }
}
