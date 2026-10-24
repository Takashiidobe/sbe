use sbe::sir::{FunctionBuilder, Signature, Type, parse_function, verify};

fn build(early_param: bool) -> String {
    let signature = Signature {
        params: vec![Type::I64],
        returns: vec![Type::I64],
    };
    let mut builder = FunctionBuilder::new("f", signature);
    let entry = builder.entry_block();
    let exit = builder.create_block();
    let carried = early_param.then(|| builder.append_block_param(exit, Type::I64).unwrap());

    builder.switch_to_block(entry).unwrap();
    let x = builder.block_params(entry).unwrap()[0];
    let one = builder.iconst(1).unwrap();
    let sum = builder.iadd(x, one).unwrap();
    let carried = match carried {
        Some(v) => v,
        None => builder.append_block_param(exit, Type::I64).unwrap(),
    };
    builder.jump(exit, &[sum]).unwrap();

    builder.switch_to_block(exit).unwrap();
    builder.ret(carried).unwrap();
    builder.finish().unwrap().to_string()
}

fn reprint(text: &str) -> String {
    let func = parse_function("roundtrip", text).unwrap();
    verify(&func).unwrap();
    func.to_string()
}

#[test]
fn text_ordered_numbering_roundtrips_exactly() {
    let text = build(false);
    assert_eq!(reprint(&text), text);
}

#[test]
fn other_numbering_settles_after_one_roundtrip() {
    let once = reprint(&build(true));
    assert_eq!(reprint(&once), once);
}
