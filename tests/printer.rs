use sbe::sir::{FunctionBuilder, Signature, Type};

#[test]
fn prints_blocks_params_and_jumps() {
    let signature = Signature {
        params: vec![Type::I64, Type::I64],
        returns: vec![Type::I64],
    };
    let mut builder = FunctionBuilder::new("add", signature);
    let entry = builder.entry_block();
    let exit = builder.create_block();
    let carried = builder.append_block_param(exit, Type::I64).unwrap();

    builder.switch_to_block(entry).unwrap();
    let [a, b] = [0, 1].map(|i| builder.block_params(entry).unwrap()[i]);
    let sum = builder.iadd(a, b).unwrap();
    builder.jump(exit, &[sum]).unwrap();

    builder.switch_to_block(exit).unwrap();
    builder.ret(carried).unwrap();

    assert_eq!(
        builder.finish().unwrap().to_string(),
        "function %add(i64, i64) -> i64 {\n\
         block0(v0: i64, v1: i64):\n    \
         v3 = iadd v0, v1\n    \
         jump block1(v3)\n\
         \n\
         block1(v2: i64):\n    \
         return v2\n\
         }\n"
    );
}

#[test]
fn prints_void_function_and_empty_jump() {
    let mut builder = FunctionBuilder::new("nop", Signature::default());
    let entry = builder.entry_block();
    let next = builder.create_block();
    builder.switch_to_block(entry).unwrap();
    builder.jump(next, &[]).unwrap();
    builder.switch_to_block(next).unwrap();
    builder.ret_values(&[]).unwrap();

    assert_eq!(
        builder.finish().unwrap().to_string(),
        "function %nop() {\n\
         block0:\n    \
         jump block1\n\
         \n\
         block1:\n    \
         return\n\
         }\n"
    );
}
