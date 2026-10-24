mod common;

use sbe::sir::parse_module;
use sbe::wasm::compile_module;
use wasmi::{Engine, Linker, Module, Store};

#[test]
fn calls_host_imports_and_module_functions() {
    let module = parse_module("calls", common::CALLS).unwrap();
    let code = compile_module(&module).unwrap().code;
    wasmparser::validate(&code).unwrap();

    let engine = Engine::default();
    let mut linker = <Linker<Vec<i64>>>::new(&engine);
    linker
        .func_wrap("env", "scale", |x: i64| common::calls_scale(x))
        .unwrap();
    linker
        .func_wrap(
            "env",
            "note",
            |mut caller: wasmi::Caller<'_, Vec<i64>>, x: i64| caller.data_mut().push(x),
        )
        .unwrap();
    let mut store = Store::new(&engine, Vec::new());
    let instance = linker
        .instantiate_and_start(&mut store, &Module::new(&engine, &code).unwrap())
        .unwrap();

    let run = instance.get_typed_func::<i64, i64>(&store, "run").unwrap();
    let fact = instance.get_typed_func::<i64, i64>(&store, "fact").unwrap();
    for n in 0..=6 {
        assert_eq!(run.call(&mut store, n).unwrap(), common::calls_run(n));
    }
    assert_eq!(fact.call(&mut store, 10).unwrap(), 3_628_800);
    assert_eq!(*store.data(), [0, 2, 4, 6, 8, 10, 12]);
}
