#![allow(dead_code)]

pub const SUM_TO: &str = include_str!("../programs/sum_to.sir");
pub const FIB: &str = include_str!("../programs/fib.sir");
pub const SWAP_N: &str = include_str!("../programs/swap_n.sir");
pub const BOTH_EDGES_MOVE: &str = include_str!("../programs/both_edges_move.sir");
pub const NESTED_LOOPS: &str = include_str!("../programs/nested_loops.sir");
pub const IRREDUCIBLE: &str = include_str!("../programs/irreducible.sir");
pub const UNREACHABLE_BLOCK: &str = include_str!("../programs/unreachable_block.sir");

pub fn sum_to(n: i64) -> i64 {
    n * (n + 1) / 2
}

pub fn fib(n: i64) -> i64 {
    let (mut a, mut b) = (0i64, 1i64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    b
}

pub fn swap_n(n: i64, x: i64, y: i64) -> i64 {
    if n % 2 == 0 { x } else { y }
}

pub fn irreducible(entry: i64, n: i64) -> i64 {
    let (mut in_one, mut left, mut acc) = (entry != 0, n, 0);
    loop {
        acc += if in_one { 1 } else { 10 };
        left -= 1;
        if left == 0 {
            return acc;
        }
        in_one = !in_one;
    }
}

pub const BINOPS: [&str; 13] = [
    "iadd", "isub", "imul", "band", "bor", "bxor", "ishl", "ushr", "sshr", "sdiv", "udiv", "srem",
    "urem",
];

pub const UNOPS: [&str; 2] = ["ineg", "bnot"];

pub const SAMPLES: [i64; 12] = [0, 1, -1, 2, 7, -7, 63, 64, 65, -64, i64::MIN, i64::MAX];

/// `None` where the op traps
pub fn binop_reference(op: &str, a: i64, b: i64) -> Option<i64> {
    Some(match op {
        "iadd" => a.wrapping_add(b),
        "isub" => a.wrapping_sub(b),
        "imul" => a.wrapping_mul(b),
        "band" => a & b,
        "bor" => a | b,
        "bxor" => a ^ b,
        "ishl" => a.wrapping_shl(b as u32),
        "ushr" => (a as u64).wrapping_shr(b as u32) as i64,
        "sshr" => a.wrapping_shr(b as u32),
        "sdiv" => a.checked_div(b)?,
        "udiv" => (a as u64).checked_div(b as u64)? as i64,
        "srem" => a.checked_rem(b)?,
        "urem" => (a as u64).checked_rem(b as u64)? as i64,
        _ => panic!("unknown binop {op}"),
    })
}

pub fn unop_reference(op: &str, a: i64) -> i64 {
    match op {
        "ineg" => a.wrapping_neg(),
        "bnot" => !a,
        _ => panic!("unknown unop {op}"),
    }
}

pub fn binop_text(op: &str) -> String {
    format!(
        "function %f(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n    v2 = {op} v0, v1\n    return v2\n}}\n"
    )
}

pub fn unop_text(op: &str) -> String {
    format!("function %f(i64) -> i64 {{\nblock0(v0: i64):\n    v1 = {op} v0\n    return v1\n}}\n")
}

pub const CONDITIONS: [&str; 10] = [
    "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
];

pub fn icmp_reference(cc: &str, a: i64, b: i64) -> bool {
    let (ua, ub) = (a as u64, b as u64);
    match cc {
        "eq" => a == b,
        "ne" => a != b,
        "slt" => a < b,
        "sle" => a <= b,
        "sgt" => a > b,
        "sge" => a >= b,
        "ult" => ua < ub,
        "ule" => ua <= ub,
        "ugt" => ua > ub,
        "uge" => ua >= ub,
        _ => panic!("unknown condition {cc}"),
    }
}

pub fn icmp_text(cc: &str) -> String {
    format!(
        "function %f(i64, i64) -> i8 {{\nblock0(v0: i64, v1: i64):\n    v2 = icmp {cc} v0, v1\n    return v2\n}}\n"
    )
}

pub const MIN_BY_SELECT: &str = "function %f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = icmp slt v0, v1\n    v3 = select v2, v0, v1\n    return v3\n}\n";

pub const I8_BLOCK_PARAM: &str = "function %f(i64, i64) -> i8 {\nblock0(v0: i64, v1: i64):\n    v2 = icmp ult v0, v1\n    jump block1(v2)\n\nblock1(v3: i8):\n    return v3\n}\n";

pub const CALLS: &str = include_str!("../programs/calls.sir");

pub fn calls_scale(x: i64) -> i64 {
    x * 2
}

pub fn calls_run(n: i64) -> i64 {
    (1..=calls_scale(n)).product::<i64>() + n
}
