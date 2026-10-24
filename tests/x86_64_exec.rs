#![cfg(all(target_arch = "x86_64", unix))]

mod common;

use memmap2::MmapMut;
use sbe::sir::{FunctionBuilder, Signature, Type, parse_function};
use sbe::x86_64::compile;

fn load(code: &[u8]) -> memmap2::Mmap {
    let mut map = MmapMut::map_anon(code.len()).unwrap();
    map.copy_from_slice(code);
    map.make_exec().unwrap()
}

fn compile_text(text: &str) -> memmap2::Mmap {
    let func = parse_function("exec", text).unwrap();
    load(&compile(&func).unwrap().code)
}

#[test]
fn add_two_params() {
    let map = compile_text(
        "function %add(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = iadd v0, v1\n    return v2\n}\n",
    );
    let add: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(-7, 3), -4);
    assert_eq!(add(i64::MAX, 1), i64::MIN);
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

    let map = load(&compile(&builder.finish().unwrap()).unwrap().code);
    let sum: extern "C" fn() -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(sum(), (1 << 40) - 10);
}

#[test]
fn six_params_use_every_argument_register() {
    let map = compile_text(
        "function %f(i64, i64, i64, i64, i64, i64) -> i64 {\n\
         block0(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64, v5: i64):\n\
         v6 = iadd v0, v1\n    v7 = iadd v6, v2\n    v8 = iadd v7, v3\n\
         v9 = iadd v8, v4\n    v10 = iadd v9, v5\n    return v10\n}\n",
    );
    let f: extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(1, 10, 100, 1000, 10_000, 100_000), 111_111);
}

#[test]
fn nine_live_values() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/filetests/x86_64/nine_live_values.sir"
    ))
    .unwrap();
    let map = compile_text(&text);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(), 36);
}

#[test]
fn unused_and_reordered_params() {
    let map = compile_text(
        "function %f(i64, i64, i64) -> i64 {\nblock0(v0: i64, v1: i64, v2: i64):\n    v3 = iadd v2, v1\n    return v3\n}\n",
    );
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(1000, 20, 3), 23);
}

#[test]
fn counting_loop() {
    let map = compile_text(common::SUM_TO);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    for n in [1, 2, 10, 1000] {
        assert_eq!(f(n), common::sum_to(n));
    }
}

#[test]
fn loop_carried_values_move_in_parallel() {
    let map = compile_text(common::FIB);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    for n in 1..=40 {
        assert_eq!(f(n), common::fib(n));
    }
}

#[test]
fn swapping_params_needs_a_cycle_break() {
    let map = compile_text(common::SWAP_N);
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    for n in 1..=5 {
        assert_eq!(f(n, 100, 200), common::swap_n(n, 100, 200));
    }
}

#[test]
fn both_branch_edges_move_params() {
    let map = compile_text(common::BOTH_EDGES_MOVE);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(5, 7), 7);
    assert_eq!(f(0, 7), 0);
}

#[test]
fn nested_loops_keep_outer_values_live() {
    let map = compile_text(common::NESTED_LOOPS);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    for (n, m) in [(1, 1), (3, 4), (7, 5), (20, 30)] {
        assert_eq!(f(n, m), n * m);
    }
}

#[test]
fn irreducible_control_flow() {
    let map = compile_text(common::IRREDUCIBLE);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    for entry in [0, 1] {
        for n in 1..=9 {
            assert_eq!(f(entry, n), common::irreducible(entry, n));
        }
    }
}

#[test]
fn unreachable_blocks_are_dropped() {
    let map = compile_text(common::UNREACHABLE_BLOCK);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(), 7);
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
    builder
        .brif(cond, then_block, &[], else_block, &[])
        .unwrap();
    builder.switch_to_block(then_block).unwrap();
    let ten = builder.iconst(10).unwrap();
    builder.jump(merge, &[ten]).unwrap();
    builder.switch_to_block(else_block).unwrap();
    let twenty = builder.iconst(20).unwrap();
    builder.jump(merge, &[twenty]).unwrap();
    builder.switch_to_block(merge).unwrap();
    builder.ret(result).unwrap();

    let map = load(&compile(&builder.finish().unwrap()).unwrap().code);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
    assert_eq!(f(1), 10);
    assert_eq!(f(0), 20);
}
