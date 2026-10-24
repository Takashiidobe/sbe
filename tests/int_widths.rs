mod common;

use common::{BINOPS, CONDITIONS, UNOPS};
use sbe::sir::parse_function;

const WIDTHS: [u32; 3] = [8, 16, 32];
const SAMPLES: [i64; 14] = [
    0,
    1,
    -1,
    2,
    7,
    -7,
    31,
    32,
    127,
    128,
    255,
    256,
    -129,
    i64::MIN,
];

type Expected = Box<dyn Fn(i64, i64) -> Option<i64>>;

struct Case {
    name: String,
    text: String,
    expected: Expected,
}

fn sx(value: i64, bits: u32) -> i64 {
    (value << (64 - bits)) >> (64 - bits)
}

fn zx(value: i64, bits: u32) -> i64 {
    ((value as u64) << (64 - bits) >> (64 - bits)) as i64
}

fn function(body: &str) -> String {
    format!(
        "function %f(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n    v2 = ireduce i{{w}} v0\n    v3 = ireduce i{{w}} v1\n{body}}}\n"
    )
}

fn binop_reference(op: &str, bits: u32, a: i64, b: i64) -> Option<i64> {
    let (sa, sb, ua, ub) = (sx(a, bits), sx(b, bits), zx(a, bits), zx(b, bits));
    let amount = (ub as u32) % bits;
    let wide = match op {
        "iadd" => sa.wrapping_add(sb),
        "isub" => sa.wrapping_sub(sb),
        "imul" => sa.wrapping_mul(sb),
        "band" => sa & sb,
        "bor" => sa | sb,
        "bxor" => sa ^ sb,
        "ishl" => ua << amount,
        "ushr" => ua >> amount,
        "sshr" => sa >> amount,
        "sdiv" => sa.checked_div(sb)?,
        "udiv" => ua.checked_div(ub)?,
        "srem" => sa.checked_rem(sb)?,
        "urem" => ua.checked_rem(ub)?,
        _ => panic!("unknown binop {op}"),
    };
    Some(sx(wide, bits))
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for bits in WIDTHS {
        let ty = format!("i{bits}");
        let mut add = |name: String, body: &str, expected: Expected| {
            cases.push(Case {
                name: format!("{name}.{ty}"),
                text: function(body).replace("{w}", &bits.to_string()),
                expected,
            });
        };

        for op in BINOPS {
            add(
                op.to_string(),
                &format!("    v4 = {op} v2, v3\n    v5 = sextend i64 v4\n    return v5\n"),
                Box::new(move |a, b| binop_reference(op, bits, a, b)),
            );
        }
        for op in UNOPS {
            add(
                op.to_string(),
                &format!("    v4 = {op} v2\n    v5 = sextend i64 v4\n    return v5\n"),
                Box::new(move |a, _| {
                    Some(sx(
                        match op {
                            "ineg" => sx(a, bits).wrapping_neg(),
                            _ => !a,
                        },
                        bits,
                    ))
                }),
            );
        }
        for cc in CONDITIONS {
            add(
                format!("icmp_{cc}"),
                &format!("    v4 = icmp {cc} v2, v3\n    v5 = uextend i64 v4\n    return v5\n"),
                Box::new(move |a, b| {
                    let (sa, sb, ua, ub) = (sx(a, bits), sx(b, bits), zx(a, bits), zx(b, bits));
                    Some(i64::from(match cc {
                        "eq" => ua == ub,
                        "ne" => ua != ub,
                        "slt" => sa < sb,
                        "sle" => sa <= sb,
                        "sgt" => sa > sb,
                        "sge" => sa >= sb,
                        "ult" => ua < ub,
                        "ule" => ua <= ub,
                        "ugt" => ua > ub,
                        _ => ua >= ub,
                    }))
                }),
            );
        }
        add(
            "uextend".into(),
            "    v4 = uextend i64 v2\n    return v4\n",
            Box::new(move |a, _| Some(zx(a, bits))),
        );
        add(
            "sextend_then_ireduce".into(),
            "    v4 = sextend i64 v2\n    v5 = ireduce i8 v4\n    v6 = uextend i64 v5\n    return v6\n",
            Box::new(|a, _| Some(zx(a, 8))),
        );
        add(
            "widen_narrow_chain".into(),
            "    v4 = uextend i64 v2\n    v5 = ireduce i32 v4\n    v6 = sextend i64 v5\n    return v6\n",
            Box::new(move |a, _| Some(sx(zx(a, bits), 32))),
        );
        add(
            "iconst".into(),
            &format!("    v4 = iconst {ty} -1\n    v5 = uextend i64 v4\n    return v5\n"),
            Box::new(move |_, _| Some(zx(-1, bits))),
        );
    }

    let garbage_cond = |body: &str| {
        format!(
            "function %f(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n    v2 = ireduce i8 v0\n{body}}}\n"
        )
    };
    let expected: fn(i64, i64) -> Option<i64> = |a, b| Some(if zx(a, 8) != 0 { b } else { a });
    cases.push(Case {
        name: "select_on_reduced_cond".into(),
        text: garbage_cond("    v3 = select v2, v1, v0\n    return v3\n"),
        expected: Box::new(expected),
    });
    cases.push(Case {
        name: "brif_on_reduced_cond".into(),
        text: garbage_cond(
            "    brif v2, block1, block2\n\nblock1:\n    return v1\n\nblock2:\n    return v0\n",
        ),
        expected: Box::new(expected),
    });
    cases
}

fn run(call: impl Fn(&Case) -> Box<dyn Fn(i64, i64) -> Option<i64>>) {
    for case in cases() {
        let actual = call(&case);
        for a in SAMPLES {
            for b in SAMPLES {
                if let Some(want) = (case.expected)(a, b) {
                    assert_eq!(actual(a, b), Some(want), "{} on ({a}, {b})", case.name);
                }
            }
        }
    }
}

#[test]
fn wasm_int_widths() {
    use sbe::wasm::compile;
    use wasmi::{Engine, Linker, Module, Store};

    run(|case| {
        let func = parse_function("f", &case.text).unwrap();
        let code = compile(&func).unwrap().code;
        wasmparser::validate(&code).unwrap();
        let engine = Engine::default();
        let module = Module::new(&engine, &code[..]).unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate_and_start(&mut store, &module)
            .unwrap();
        let f = instance
            .get_typed_func::<(i64, i64), i64>(&store, "f")
            .unwrap();
        let store = std::cell::RefCell::new(store);
        Box::new(move |a, b| f.call(&mut *store.borrow_mut(), (a, b)).ok())
    });
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn x86_64_int_widths() {
    use memmap2::MmapMut;
    use sbe::x86_64::compile;

    run(|case| {
        let func = parse_function("f", &case.text).unwrap();
        let code = compile(&func).unwrap().code;
        let mut map = MmapMut::map_anon(code.len()).unwrap();
        map.copy_from_slice(&code);
        let map = map.make_exec().unwrap();
        Box::new(move |a, b| {
            let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(map.as_ptr()) };
            Some(f(a, b))
        })
    });
}
