mod common;

use sbe::sir::{FunctionBuilder, IntCC, Signature, Type, parse_function};
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
        "function %pair(i64) -> i64, i64 {\nblock0(v0: i64):\n    v1 = iconst i64 1\n    v2 = iadd v0, v1\n    return v2, v0\n}\n",
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

fn typed<P, R>(code: &[u8], name: &str) -> (Store<()>, wasmi::TypedFunc<P, R>)
where
    P: wasmi::WasmParams,
    R: wasmi::WasmResults,
{
    let (store, instance) = instantiate(code);
    let func = instance.get_typed_func::<P, R>(&store, name).unwrap();
    (store, func)
}

#[test]
fn counting_loop() {
    let (mut store, f) = typed::<i64, i64>(&compile_text(common::SUM_TO), "sum_to");
    for n in [1, 2, 10, 1000] {
        assert_eq!(f.call(&mut store, n).unwrap(), common::sum_to(n));
    }
}

#[test]
fn loop_carried_values_move_in_parallel() {
    let (mut store, f) = typed::<i64, i64>(&compile_text(common::FIB), "fib");
    for n in 1..=40 {
        assert_eq!(f.call(&mut store, n).unwrap(), common::fib(n));
    }
}

#[test]
fn swapping_params_is_simultaneous() {
    let (mut store, f) = typed::<(i64, i64, i64), i64>(&compile_text(common::SWAP_N), "swap_n");
    for n in 1..=5 {
        assert_eq!(
            f.call(&mut store, (n, 100, 200)).unwrap(),
            common::swap_n(n, 100, 200)
        );
    }
}

#[test]
fn both_branch_edges_move_params() {
    let (mut store, f) = typed::<(i64, i64), i64>(&compile_text(common::BOTH_EDGES_MOVE), "pick");
    assert_eq!(f.call(&mut store, (5, 7)).unwrap(), 7);
    assert_eq!(f.call(&mut store, (0, 7)).unwrap(), 0);
}

#[test]
fn nested_loops() {
    let (mut store, f) = typed::<(i64, i64), i64>(&compile_text(common::NESTED_LOOPS), "product");
    for (n, m) in [(1, 1), (3, 4), (7, 5), (20, 30)] {
        assert_eq!(f.call(&mut store, (n, m)).unwrap(), n * m);
    }
}

#[test]
fn irreducible_control_flow() {
    let (mut store, f) =
        typed::<(i64, i64), i64>(&compile_text(common::IRREDUCIBLE), "irreducible");
    for entry in [0, 1] {
        for n in 1..=9 {
            assert_eq!(
                f.call(&mut store, (entry, n)).unwrap(),
                common::irreducible(entry, n)
            );
        }
    }
}

#[test]
fn unreachable_blocks_are_dropped() {
    let (mut store, f) = typed::<(), i64>(&compile_text(common::UNREACHABLE_BLOCK), "f");
    assert_eq!(f.call(&mut store, ()).unwrap(), 7);
}

#[test]
fn builder_diamond() {
    let signature = Signature {
        params: vec![Type::I64],
        returns: vec![Type::I64],
    };
    let mut builder = FunctionBuilder::new("pick", signature);
    let entry = builder.entry_block();
    let (then_block, else_block, merge) = (
        builder.create_block(),
        builder.create_block(),
        builder.create_block(),
    );
    let result = builder.append_block_param(merge, Type::I64).unwrap();
    let cond = builder.block_params(entry).unwrap()[0];

    builder.switch_to_block(entry).unwrap();
    let zero = builder.iconst(0).unwrap();
    let nonzero = builder.icmp(IntCC::Ne, cond, zero).unwrap();
    builder
        .brif(nonzero, then_block, &[], else_block, &[])
        .unwrap();
    builder.switch_to_block(then_block).unwrap();
    let ten = builder.iconst(10).unwrap();
    builder.jump(merge, &[ten]).unwrap();
    builder.switch_to_block(else_block).unwrap();
    let twenty = builder.iconst(20).unwrap();
    builder.jump(merge, &[twenty]).unwrap();
    builder.switch_to_block(merge).unwrap();
    builder.ret(result).unwrap();

    let code = compile(&builder.finish().unwrap()).unwrap().code;
    let (mut store, f) = typed::<i64, i64>(&code, "pick");
    assert_eq!(f.call(&mut store, 1).unwrap(), 10);
    assert_eq!(f.call(&mut store, 0).unwrap(), 20);
}

#[test]
fn binary_ops_match_reference() {
    for op in common::BINOPS {
        let code = compile_text(&common::binop_text(op));
        let (mut store, f) = typed::<(i64, i64), i64>(&code, "f");
        for a in common::SAMPLES {
            for b in common::SAMPLES {
                let got = f.call(&mut store, (a, b)).ok();
                assert_eq!(got, common::binop_reference(op, a, b), "{op} {a} {b}");
            }
        }
    }
}

#[test]
fn unary_ops_match_reference() {
    for op in common::UNOPS {
        let code = compile_text(&common::unop_text(op));
        let (mut store, f) = typed::<i64, i64>(&code, "f");
        for a in common::SAMPLES {
            assert_eq!(
                f.call(&mut store, a).unwrap(),
                common::unop_reference(op, a),
                "{op} {a}"
            );
        }
    }
}

#[test]
fn icmp_matches_reference() {
    for cc in common::CONDITIONS {
        let code = compile_text(&common::icmp_text(cc));
        let (mut store, f) = typed::<(i64, i64), i32>(&code, "f");
        for a in common::SAMPLES {
            for b in common::SAMPLES {
                let got = f.call(&mut store, (a, b)).unwrap();
                assert_eq!(got, common::icmp_reference(cc, a, b) as i32, "{cc} {a} {b}");
            }
        }
    }
}

#[test]
fn select_picks_by_condition() {
    let code = compile_text(common::MIN_BY_SELECT);
    let (mut store, f) = typed::<(i64, i64), i64>(&code, "f");
    for a in common::SAMPLES {
        for b in common::SAMPLES {
            assert_eq!(f.call(&mut store, (a, b)).unwrap(), a.min(b), "min {a} {b}");
        }
    }
}

#[test]
fn i8_block_param_carries_comparison() {
    let code = compile_text(common::I8_BLOCK_PARAM);
    let (mut store, f) = typed::<(i64, i64), i32>(&code, "f");
    assert_eq!(f.call(&mut store, (1, 2)).unwrap(), 1);
    assert_eq!(f.call(&mut store, (2, 1)).unwrap(), 0);
    assert_eq!(f.call(&mut store, (-1, 1)).unwrap(), 0);
}

#[test]
fn constant_folding_preserves_wrapping_arithmetic() {
    for (ty, expected) in [
        ("i8", 254),
        ("i16", 65534),
        ("i32", 4294967294),
        ("i64", -2),
    ] {
        let source = format!(
            "function %fold() -> i64 {{
block0:
 v0 = iconst {ty} -1
 v1 = iconst {ty} 2
 v2 = imul v0, v1
 {}
}}",
            if ty == "i64" {
                "return v2".to_owned()
            } else {
                "v3 = uextend i64 v2
 return v3"
                    .to_owned()
            }
        );
        let mut func = parse_function("fold", &source).unwrap();
        sbe::sopt::PassManager::default_pipeline()
            .verify_between_passes(true)
            .run(&mut func)
            .unwrap();
        let (mut store, instance) = instantiate(&compile(&func).unwrap().code);
        let fold = instance.get_typed_func::<(), i64>(&store, "fold").unwrap();
        assert_eq!(fold.call(&mut store, ()).unwrap(), expected);
    }
}

#[test]
fn egraph_preserves_loop_carried_values() {
    for (source, name, reference) in [
        (common::SUM_TO, "sum_to", common::sum_to as fn(i64) -> i64),
        (common::FIB, "fib", common::fib as fn(i64) -> i64),
    ] {
        let mut func = parse_function(name, source).unwrap();
        sbe::sopt::PassManager::default_pipeline()
            .verify_between_passes(true)
            .run(&mut func)
            .unwrap();
        let (mut store, instance) = instantiate(&compile(&func).unwrap().code);
        let run = instance.get_typed_func::<i64, i64>(&store, name).unwrap();
        for n in 1..30 {
            assert_eq!(run.call(&mut store, n).unwrap(), reference(n));
        }
    }
}

#[test]
fn egraph_preserves_unused_trapping_operations() {
    let mut func = parse_function(
        "trap",
        "
function %trap() -> i64 {
block0:
    v0 = iconst i64 9
    v1 = iconst i64 0
    v2 = sdiv v0, v1
    return v0
}
",
    )
    .unwrap();
    sbe::sopt::PassManager::default_pipeline()
        .verify_between_passes(true)
        .run(&mut func)
        .unwrap();
    let (mut store, instance) = instantiate(&compile(&func).unwrap().code);
    let run = instance.get_typed_func::<(), i64>(&store, "trap").unwrap();
    assert!(run.call(&mut store, ()).is_err());
}
