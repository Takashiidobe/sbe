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
