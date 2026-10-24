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

#[test]
fn memory_and_shadow_stack() {
    let module = parse_module("memory", common::MEMORY).unwrap();
    let code = compile_module(&module).unwrap().code;
    wasmparser::validate(&code).unwrap();

    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &Module::new(&engine, &code).unwrap())
        .unwrap();

    let unary = |name: &str| instance.get_typed_func::<i64, i64>(&store, name).unwrap();
    let (triple, widths, squares, rec) = (
        unary("triple"),
        unary("widths"),
        unary("squares"),
        unary("rec"),
    );
    for x in common::SAMPLES {
        assert_eq!(
            triple.call(&mut store, x).unwrap(),
            common::memory_triple(x)
        );
        assert_eq!(
            widths.call(&mut store, x).unwrap(),
            common::memory_widths(x)
        );
    }
    for n in 0..=8 {
        assert_eq!(
            squares.call(&mut store, n).unwrap(),
            common::memory_squares(n)
        );
    }
    for n in [0, 1, 5, 100] {
        assert_eq!(rec.call(&mut store, n).unwrap(), common::memory_rec(n));
    }

    let memory = instance.get_memory(&store, "memory").unwrap();
    memory
        .write(&mut store, 1000, &[1, 0, 0, 0, 0xfe, 0xff, 0xff, 0xff])
        .unwrap();
    let read = instance
        .get_typed_func::<i64, i64>(&store, "read_i32s")
        .unwrap();
    assert_eq!(read.call(&mut store, 1000).unwrap(), -1);
    let write = instance
        .get_typed_func::<(i64, i32), ()>(&store, "write_byte")
        .unwrap();
    write.call(&mut store, (1001, 0x1ff)).unwrap();
    let mut byte = [0u8];
    memory.read(&store, 1001, &mut byte).unwrap();
    assert_eq!(byte, [0xff]);
}
