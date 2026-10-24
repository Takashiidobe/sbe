use sbe::sir::{FunctionBuilder, Signature, Type, parse_function};
use sbe::wasm::compile;
use wasmi::{Engine, Instance, Linker, Module, Store};

fn instantiate(code: &[u8]) -> (Store<()>, Instance) {
    wasmparser::validate(code).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, code).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &module)
        .unwrap();
    (store, instance)
}

fn compile_text(text: &str) -> Vec<u8> {
    compile(&parse_function("exec", text).unwrap())
        .unwrap()
        .code
}

#[test]
fn add_two_params() {
    let code = compile_text(
        "function %add(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = iadd v0, v1\n    return v2\n}\n",
    );
    let (mut store, instance) = instantiate(&code);
    let add = instance
        .get_typed_func::<(i64, i64), i64>(&store, "add")
        .unwrap();
    assert_eq!(add.call(&mut store, (2, 3)).unwrap(), 5);
    assert_eq!(add.call(&mut store, (-7, 3)).unwrap(), -4);
    assert_eq!(add.call(&mut store, (i64::MAX, 1)).unwrap(), i64::MIN);
}

#[test]
fn builder_function_with_constants() {
    let signature = Signature {
        params: vec![],
        returns: vec![Type::I64],
    };
    let mut builder = FunctionBuilder::new("sum", signature);
    let entry = builder.entry_block();
    builder.switch_to_block(entry).unwrap();
    let a = builder.iconst(10).unwrap();
    let b = builder.iconst(-20).unwrap();
    let c = builder.iconst(1 << 40).unwrap();
    let ab = builder.iadd(a, b).unwrap();
    let sum = builder.iadd(ab, c).unwrap();
    builder.ret(sum).unwrap();

    let code = compile(&builder.finish().unwrap()).unwrap().code;
    let (mut store, instance) = instantiate(&code);
    let sum = instance.get_typed_func::<(), i64>(&store, "sum").unwrap();
    assert_eq!(sum.call(&mut store, ()).unwrap(), (1 << 40) - 10);
}

#[test]
fn multiple_return_values() {
    let code = compile_text(
        "function %pair(i64) -> i64, i64 {\nblock0(v0: i64):\n    v1 = iconst 1\n    v2 = iadd v0, v1\n    return v2, v0\n}\n",
    );
    let (mut store, instance) = instantiate(&code);
    let pair = instance
        .get_typed_func::<i64, (i64, i64)>(&store, "pair")
        .unwrap();
    assert_eq!(pair.call(&mut store, 41).unwrap(), (42, 41));
}

#[test]
fn void_function() {
    let code = compile_text("function %nop() {\nblock0:\n    return\n}\n");
    let (mut store, instance) = instantiate(&code);
    let nop = instance.get_typed_func::<(), ()>(&store, "nop").unwrap();
    nop.call(&mut store, ()).unwrap();
}
