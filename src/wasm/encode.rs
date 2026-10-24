use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection,
    IndirectNameMap, Instruction, Module, NameMap, NameSection, TypeSection, ValType,
};

use super::error::CompileError;
use crate::sir::{Function, InstData, Terminator, Value, verify};

pub struct Compiled {
    pub code: Vec<u8>,
    pub wat: String,
}

/// every sir value lives in its own wasm local; params are the first locals
pub fn compile(func: &Function) -> Result<Compiled, CompileError> {
    verify(func)?;
    if func.blocks().count() != 1 {
        return Err(CompileError::MultipleBlocks);
    }

    let block = func.block(func.entry_block());
    let num_params = block.params().len();

    let mut local_of = vec![0u32; func.num_values()];
    let mut local_values: Vec<Value> = Vec::new();
    for &param in block.params() {
        local_of[param.index()] = local_values.len() as u32;
        local_values.push(param);
    }
    for &inst in block.insts() {
        let result = func.inst_result(inst);
        local_of[result.index()] = local_values.len() as u32;
        local_values.push(result);
    }

    let mut body = WasmFunction::new([((local_values.len() - num_params) as u32, ValType::I64)]);
    for &inst in block.insts() {
        let result = local_of[func.inst_result(inst).index()];
        match func.inst(inst) {
            InstData::Iconst { value } => {
                body.instruction(&Instruction::I64Const(*value));
            }
            InstData::Iadd { args: [a, b] } => {
                body.instruction(&Instruction::LocalGet(local_of[a.index()]));
                body.instruction(&Instruction::LocalGet(local_of[b.index()]));
                body.instruction(&Instruction::I64Add);
            }
        }
        body.instruction(&Instruction::LocalSet(result));
    }
    match block.terminator() {
        Some(Terminator::Return { values }) => {
            for value in values {
                body.instruction(&Instruction::LocalGet(local_of[value.index()]));
            }
        }
        Some(Terminator::Jump { .. }) | None => return Err(CompileError::MultipleBlocks),
    }
    body.instruction(&Instruction::End);

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
