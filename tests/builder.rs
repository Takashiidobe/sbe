use sbe::sir::{BuilderError, FunctionBuilder, Signature, Type, Value};

fn i64_sig(params: usize, returns: usize) -> Signature {
    Signature {
        params: vec![Type::I64; params],
        returns: vec![Type::I64; returns],
    }
}

#[test]
fn builds_constant_add() {
    let mut builder = FunctionBuilder::new("sum", i64_sig(0, 1));
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();

    let a = builder.iconst(10).unwrap();
    let b = builder.iconst(20).unwrap();
    let sum = builder.iadd(a, b).unwrap();
    builder.ret(sum).unwrap();

    let func = builder.finish().unwrap();
    assert_eq!(
        func.to_string(),
        "function %sum() -> i64 {\n\
         block0:\n    \
         v0 = iconst i64 10\n    \
         v1 = iconst i64 20\n    \
         v2 = iadd v0, v1\n    \
         return v2\n\
         }\n"
    );
}

#[test]
fn builds_multi_block_with_block_params() {
    let mut builder = FunctionBuilder::new("pass", i64_sig(1, 1));
    let entry = builder.entry_block();
    let exit = builder.create_block();
    let carried = builder.append_block_param(exit, Type::I64).unwrap();

    builder.switch_to_block(entry).unwrap();
    let x = builder.block_params(entry).unwrap()[0];
    builder.jump(exit, &[x]).unwrap();

    builder.switch_to_block(exit).unwrap();
    builder.ret(carried).unwrap();

    assert!(builder.finish().is_ok());
}

#[test]
fn emitting_without_a_current_block_fails() {
    let mut builder = FunctionBuilder::new("f", i64_sig(0, 0));
    assert_eq!(builder.iconst(1), Err(BuilderError::NoCurrentBlock));
}

#[test]
fn emitting_after_a_terminator_fails() {
    let mut builder = FunctionBuilder::new("f", i64_sig(0, 1));
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();
    let v = builder.iconst(1).unwrap();
    builder.ret(v).unwrap();

    assert_eq!(builder.iconst(2), Err(BuilderError::BlockTerminated(entry)));
    assert_eq!(builder.ret(v), Err(BuilderError::BlockTerminated(entry)));
}

#[test]
fn unknown_values_and_blocks_are_rejected() {
    let mut other = FunctionBuilder::new("other", i64_sig(0, 0));
    let foreign_block2 = other.create_block();

    let mut builder = FunctionBuilder::new("f", i64_sig(0, 1));
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();
    let v = builder.iconst(1).unwrap();
    let bogus = Value::from_index(99);

    assert_eq!(
        builder.iadd(v, bogus),
        Err(BuilderError::UnknownValue(bogus))
    );
    assert_eq!(builder.ret(bogus), Err(BuilderError::UnknownValue(bogus)));
    assert_eq!(
        builder.switch_to_block(foreign_block2),
        Err(BuilderError::UnknownBlock(foreign_block2))
    );
    assert_eq!(
        builder.jump(foreign_block2, &[]),
        Err(BuilderError::UnknownBlock(foreign_block2))
    );
}

#[test]
fn finish_rejects_unterminated_blocks() {
    let mut builder = FunctionBuilder::new("f", i64_sig(0, 0));
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();
    builder.iconst(1).unwrap();

    let err = builder.finish().unwrap_err();
    assert_eq!(err.errors.len(), 1);
    assert_eq!(err.to_string(), "function %f failed verification");
}

#[test]
fn builds_calls_to_declared_callees() {
    let mut builder = FunctionBuilder::new("f", i64_sig(1, 1));
    let double = builder.declare_callee("double", i64_sig(1, 1));
    let log = builder.declare_callee("log", i64_sig(1, 0));
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();

    let x = builder.block_params(entry).unwrap()[0];
    let doubled = builder.call(double, &[x]).unwrap().unwrap();
    assert_eq!(builder.call(log, &[doubled]).unwrap(), None);
    builder.ret(doubled).unwrap();

    let func = builder.finish().unwrap();
    assert_eq!(
        func.to_string(),
        "function %f(i64) -> i64 {\n    \
         fn0 = %double(i64) -> i64\n    \
         fn1 = %log(i64)\n\n\
         block0(v0: i64):\n    \
         v1 = call fn0(v0)\n    \
         call fn1(v1)\n    \
         return v1\n\
         }\n"
    );
}

#[test]
fn rejects_calls_to_undeclared_callees() {
    let mut other = FunctionBuilder::new("other", i64_sig(0, 0));
    let foreign = other.declare_callee("g", i64_sig(0, 0));

    let mut builder = FunctionBuilder::new("f", i64_sig(0, 0));
    builder.switch_to_block(builder.entry_block()).unwrap();
    assert_eq!(
        builder.call(foreign, &[]),
        Err(BuilderError::UnknownCallee(foreign))
    );
}
