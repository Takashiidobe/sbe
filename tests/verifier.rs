use sbe::sir::{
    BinOp, Block, Function, InstData, Signature, Terminator, Type, Use, Value, VerifyError, verify,
};

fn sig(params: usize, returns: usize) -> Signature {
    Signature {
        params: vec![Type::I64; params],
        returns: vec![Type::I64; returns],
    }
}

fn errors(func: &Function) -> Vec<VerifyError> {
    verify(func).err().map(|e| e.errors).unwrap_or_default()
}

#[test]
fn valid_function_passes() {
    let mut func = Function::new("add", sig(2, 1));
    let entry = func.entry_block();
    let [a, b] = [0, 1].map(|i| func.block(entry).params()[i]);
    let sum = func.push_inst(
        entry,
        InstData::Binary {
            op: BinOp::Iadd,
            args: [a, b],
        },
    );
    func.set_terminator(entry, Terminator::Return { values: vec![sum] });
    assert_eq!(errors(&func), []);
}

#[test]
fn missing_terminator() {
    let mut func = Function::new("f", sig(0, 0));
    let entry = func.entry_block();
    let next = func.create_block();
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: next,
            args: vec![],
        },
    );
    assert_eq!(errors(&func), [VerifyError::MissingTerminator(next)]);
}

#[test]
fn entry_params_must_match_signature() {
    let mut func = Function::new("f", sig(1, 0));
    let entry = func.entry_block();
    func.append_block_param(entry, Type::I64);
    func.set_terminator(entry, Terminator::Return { values: vec![] });
    assert_eq!(errors(&func), [VerifyError::EntryParams]);
}

#[test]
fn use_before_def_in_same_block() {
    let mut func = Function::new("f", sig(0, 1));
    let entry = func.entry_block();
    let later = Value::from_index(1);
    let first = func.push_inst(
        entry,
        InstData::Binary {
            op: BinOp::Iadd,
            args: [later, later],
        },
    );
    let defined_late = func.push_inst(
        entry,
        InstData::Iconst {
            ty: Type::I64,
            value: 1,
        },
    );
    assert_eq!(defined_late, later);
    func.set_terminator(
        entry,
        Terminator::Return {
            values: vec![first],
        },
    );

    let inst0 = func.block(entry).insts()[0];
    assert_eq!(
        errors(&func),
        [
            VerifyError::NotDominated {
                user: Use::Inst(inst0),
                value: later
            },
            VerifyError::NotDominated {
                user: Use::Inst(inst0),
                value: later
            },
        ]
    );
}

#[test]
fn use_of_value_from_non_dominating_block() {
    let mut func = Function::new("f", sig(0, 1));
    let entry = func.entry_block();
    let skipped = func.create_block();
    let target = func.create_block();
    let hidden = func.push_inst(
        skipped,
        InstData::Iconst {
            ty: Type::I64,
            value: 1,
        },
    );
    func.set_terminator(
        skipped,
        Terminator::Jump {
            dest: target,
            args: vec![],
        },
    );
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: target,
            args: vec![],
        },
    );
    func.set_terminator(
        target,
        Terminator::Return {
            values: vec![hidden],
        },
    );

    assert_eq!(
        errors(&func),
        [VerifyError::NotDominated {
            user: Use::Terminator(target),
            value: hidden
        }]
    );
}

#[test]
fn definition_dominating_across_blocks_is_fine() {
    let mut func = Function::new("f", sig(0, 1));
    let entry = func.entry_block();
    let next = func.create_block();
    let v = func.push_inst(
        entry,
        InstData::Iconst {
            ty: Type::I64,
            value: 1,
        },
    );
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: next,
            args: vec![],
        },
    );
    func.set_terminator(next, Terminator::Return { values: vec![v] });
    assert_eq!(errors(&func), []);
}

#[test]
fn loop_back_edge_with_block_params_is_fine() {
    let mut func = Function::new("f", sig(1, 1));
    let entry = func.entry_block();
    let head = func.create_block();
    let acc = func.append_block_param(head, Type::I64);
    let x = func.block(entry).params()[0];
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: head,
            args: vec![x],
        },
    );
    let next = func.push_inst(
        head,
        InstData::Binary {
            op: BinOp::Iadd,
            args: [acc, acc],
        },
    );
    func.set_terminator(
        head,
        Terminator::Jump {
            dest: head,
            args: vec![next],
        },
    );
    assert_eq!(errors(&func), []);
}

#[test]
fn unknown_value_use() {
    let mut func = Function::new("f", sig(0, 1));
    let entry = func.entry_block();
    let bogus = Value::from_index(42);
    func.set_terminator(
        entry,
        Terminator::Return {
            values: vec![bogus],
        },
    );
    assert_eq!(
        errors(&func),
        [VerifyError::UnknownValue {
            user: Use::Terminator(entry),
            value: bogus
        }]
    );
}

#[test]
fn return_arity_must_match_signature() {
    let mut func = Function::new("f", sig(0, 1));
    let entry = func.entry_block();
    func.set_terminator(entry, Terminator::Return { values: vec![] });
    assert_eq!(errors(&func), [VerifyError::ReturnArity(entry)]);
}

#[test]
fn jump_arity_must_match_dest_params() {
    let mut func = Function::new("f", sig(0, 0));
    let entry = func.entry_block();
    let dest = func.create_block();
    func.append_block_param(dest, Type::I64);
    func.set_terminator(entry, Terminator::Jump { dest, args: vec![] });
    func.set_terminator(dest, Terminator::Return { values: vec![] });
    assert_eq!(
        errors(&func),
        [VerifyError::JumpArity { from: entry, dest }]
    );
}

#[test]
fn jump_to_unknown_block() {
    let mut func = Function::new("f", sig(0, 0));
    let entry = func.entry_block();
    let dest = Block::from_index(7);
    func.set_terminator(entry, Terminator::Jump { dest, args: vec![] });
    assert_eq!(
        errors(&func),
        [VerifyError::UnknownBlock { from: entry, dest }]
    );
}

#[test]
fn jump_to_entry_block() {
    let mut func = Function::new("f", sig(0, 0));
    let entry = func.entry_block();
    func.set_terminator(
        entry,
        Terminator::Jump {
            dest: entry,
            args: vec![],
        },
    );
    assert_eq!(errors(&func), [VerifyError::JumpToEntry { from: entry }]);
}
