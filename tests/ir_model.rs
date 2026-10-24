use sbe::sir::{Function, InstData, Signature, Terminator, Type, ValueDef};

fn add_function() -> Function {
    let signature = Signature {
        params: vec![Type::I64, Type::I64],
        returns: vec![Type::I64],
    };
    let mut func = Function::new("add", signature);
    let entry = func.entry_block();
    let [a, b] = [0, 1].map(|i| func.block(entry).params()[i]);
    let sum = func.push_inst(entry, InstData::Iadd { args: [a, b] });
    func.set_terminator(entry, Terminator::Return { values: vec![sum] });
    func
}

#[test]
fn entry_block_params_are_function_params() {
    let func = add_function();
    let entry = func.entry_block();
    let params = func.block(entry).params();

    assert_eq!(params.len(), 2);
    for (i, &p) in params.iter().enumerate() {
        assert_eq!(func.value_type(p), Type::I64);
        assert_eq!(func.value_def(p), ValueDef::Param(entry, i));
    }
}

#[test]
fn instruction_results_are_single_definitions() {
    let func = add_function();
    let block = func.block(func.entry_block());

    let [inst] = block.insts() else {
        panic!("expected one inst")
    };
    let result = func.inst_result(*inst);
    assert_eq!(func.value_def(result), ValueDef::Result(*inst));
    assert_eq!(func.value_type(result), Type::I64);
    assert_eq!(func.num_values(), 3);
    assert_eq!(
        block.terminator(),
        Some(&Terminator::Return {
            values: vec![result]
        })
    );
}

#[test]
fn extra_blocks_take_params_instead_of_phis() {
    let mut func = add_function();
    let entry = func.entry_block();
    let one = func.push_inst(entry, InstData::Iconst { value: 1 });
    let next = func.create_block();
    let carried = func.append_block_param(next, Type::I64);
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: next,
            args: vec![one],
        },
    );

    assert_eq!(func.blocks().count(), 2);
    assert_eq!(func.block(next).params(), [carried]);
    assert_eq!(func.value_def(carried), ValueDef::Param(next, 0));
    assert_eq!(func.block(next).terminator(), None);
}
