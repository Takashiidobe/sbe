use crate::sir::{BinOp, Type};

use super::{EGraph, Expr, Rewrite};

pub struct ConstantFold;

impl Rewrite for ConstantFold {
    fn priority(&self) -> u32 {
        0
    }

    fn accepts(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Binary { .. })
    }

    fn rewrite(&self, graph: &EGraph, expr: &Expr) -> Option<Expr> {
        let Expr::Binary {
            ty,
            op,
            args: [a, b],
        } = *expr
        else {
            return None;
        };
        let value = fold(op, ty, graph.constant(a)?, graph.constant(b)?)?;
        Some(Expr::Iconst { ty, value })
    }
}

fn fold(op: BinOp, ty: Type, a: u64, b: u64) -> Option<u64> {
    let value = match op {
        BinOp::Iadd => a.wrapping_add(b),
        BinOp::Isub => a.wrapping_sub(b),
        BinOp::Imul => a.wrapping_mul(b),
        BinOp::Band => a & b,
        BinOp::Bor => a | b,
        BinOp::Bxor => a ^ b,
        _ => return None,
    };
    Some(ty.truncate(value as i64))
}
