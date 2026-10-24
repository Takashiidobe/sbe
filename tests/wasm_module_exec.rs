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

#[test]
fn switch_trap_and_irreducible_dispatch() {
    let module = parse_module("switch", common::SWITCH).unwrap();
    let code = compile_module(&module).unwrap().code;
    wasmparser::validate(&code).unwrap();

    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &Module::new(&engine, &code).unwrap())
        .unwrap();

    let classify = instance
        .get_typed_func::<i64, i64>(&store, "classify")
        .unwrap();
    let by_byte = instance
        .get_typed_func::<i64, i64>(&store, "by_byte")
        .unwrap();
    for x in common::SAMPLES.into_iter().chain([2, 3, 4, 6]) {
        assert_eq!(
            classify.call(&mut store, x).unwrap(),
            common::switch_classify(x)
        );
    }
    for x in common::SWITCH_BYTE_SAMPLES {
        assert_eq!(
            by_byte.call(&mut store, x).unwrap(),
            common::switch_by_byte(x)
        );
    }

    let ping = instance
        .get_typed_func::<(i64, i64), i64>(&store, "ping")
        .unwrap();
    for sel in [0, 1, 2, -1] {
        for n in [-3, 0, 1, 2, 7] {
            assert_eq!(
                ping.call(&mut store, (sel, n)).unwrap(),
                common::switch_ping(sel, n)
            );
        }
    }

    let div = instance
        .get_typed_func::<(i64, i64), i64>(&store, "checked_div")
        .unwrap();
    assert_eq!(div.call(&mut store, (-9, 2)).unwrap(), -4);
    assert!(div.call(&mut store, (1, 0)).is_err());
}

#[test]
fn floats_match_reference() {
    let module = parse_module("float", common::FLOAT).unwrap();
    let code = compile_module(&module).unwrap().code;
    wasmparser::validate(&code).unwrap();

    let engine = Engine::default();
    let mut linker = <Linker<()>>::new(&engine);
    linker
        .func_wrap("env", "hyp", |x: f64, n: i64, y: f32| {
            common::float_hyp(x, n, y)
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate_and_start(&mut store, &Module::new(&engine, &code).unwrap())
        .unwrap();

    let same = |a: f64, b: f64| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan());
    let axpy = instance
        .get_typed_func::<(f64, f64, f64), f64>(&store, "axpy")
        .unwrap();
    let sub_div = instance
        .get_typed_func::<(f64, f64), f64>(&store, "sub_div")
        .unwrap();
    let cmp_all = instance
        .get_typed_func::<(f64, f64), i64>(&store, "cmp_all")
        .unwrap();
    let fmax = instance
        .get_typed_func::<(f64, f64), f64>(&store, "max_of")
        .unwrap();
    for a in common::FLOAT_SAMPLES {
        for b in common::FLOAT_SAMPLES {
            assert_eq!(
                cmp_all.call(&mut store, (a, b)).unwrap(),
                common::float_cmp_all(a, b)
            );
            assert!(same(sub_div.call(&mut store, (a, b)).unwrap(), (a - b) / b));
            assert!(same(
                fmax.call(&mut store, (a, b)).unwrap(),
                if a > b { a } else { b }
            ));
            assert!(same(
                axpy.call(&mut store, (a, b, 1.0)).unwrap(),
                a * b + 1.0
            ));
        }
    }

    let mean32 = instance
        .get_typed_func::<(f32, f32), f32>(&store, "mean32")
        .unwrap();
    assert_eq!(mean32.call(&mut store, (1.0, 4.0)).unwrap(), 2.5);
    let cmp32 = instance
        .get_typed_func::<(f32, f32), i64>(&store, "cmp_all32")
        .unwrap();
    for (a, b) in [(1.0, 2.0), (2.0, 1.0), (1.0, 1.0), (f32::NAN, 1.0)] {
        assert_eq!(
            cmp32.call(&mut store, (a, b)).unwrap(),
            common::float_cmp_all32(a, b)
        );
    }

    let trunc = instance
        .get_typed_func::<f64, i64>(&store, "to_int")
        .unwrap();
    for x in [0.0, -0.0, 1.9, -1.9, 123456.789, -1e12] {
        assert_eq!(trunc.call(&mut store, x).unwrap(), x as i64);
    }
    let trunc32 = instance
        .get_typed_func::<f32, i32>(&store, "to_int32")
        .unwrap();
    assert_eq!(trunc32.call(&mut store, -7.9).unwrap(), -7);
    let roundtrip = instance
        .get_typed_func::<i64, i64>(&store, "roundtrip")
        .unwrap();
    for n in [0, 1, -1, 1 << 52, -(1 << 40)] {
        assert_eq!(roundtrip.call(&mut store, n).unwrap(), n);
    }
    let small = instance
        .get_typed_func::<i32, f32>(&store, "small_to_float")
        .unwrap();
    assert_eq!(
        small.call(&mut store, 70000).unwrap(),
        f32::from(70000u32 as u16 as i16)
    );

    let widen = instance
        .get_typed_func::<f32, f64>(&store, "widen")
        .unwrap();
    assert_eq!(widen.call(&mut store, 0.1).unwrap(), f64::from(0.1f32));
    let narrow = instance
        .get_typed_func::<f64, f32>(&store, "narrow")
        .unwrap();
    assert_eq!(narrow.call(&mut store, 0.1).unwrap(), 0.1f64 as f32);
    let bits = instance.get_typed_func::<f64, i64>(&store, "bits").unwrap();
    let from_bits = instance
        .get_typed_func::<i64, f64>(&store, "from_bits")
        .unwrap();
    for x in common::FLOAT_SAMPLES {
        assert_eq!(bits.call(&mut store, x).unwrap(), x.to_bits() as i64);
        assert_eq!(
            from_bits
                .call(&mut store, x.to_bits() as i64)
                .unwrap()
                .to_bits(),
            x.to_bits()
        );
    }

    let fsum = instance.get_typed_func::<i64, f64>(&store, "fsum").unwrap();
    for n in [0, 1, 10, 100] {
        assert_eq!(fsum.call(&mut store, n).unwrap(), common::float_fsum(n));
    }
    let mixed = instance
        .get_typed_func::<(i64, f64, i32, f32), f64>(&store, "mixed")
        .unwrap();
    assert_eq!(
        mixed.call(&mut store, (3, 0.5, -4, 1.25)).unwrap(),
        common::float_mixed(3, 0.5, -4, 1.25)
    );
    let via_memory = instance
        .get_typed_func::<(f64, f32), f64>(&store, "via_memory")
        .unwrap();
    assert_eq!(via_memory.call(&mut store, (1.5, 2.25)).unwrap(), 3.75);
    let call_hyp = instance
        .get_typed_func::<(f64, i64, f32), f64>(&store, "call_hyp")
        .unwrap();
    assert_eq!(
        call_hyp.call(&mut store, (1.5, 2, 0.25)).unwrap(),
        common::float_hyp(1.5, 2, 0.25) + 1.0
    );
}
