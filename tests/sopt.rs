use std::cell::RefCell;
use std::rc::Rc;

use sbe::sir::{BinOp, Function, InstData, Signature, Terminator, Value, parse_function};
use sbe::sopt::{NullPass, Pass, PassManager};

fn identity() -> Function {
    parse_function(
        "sopt",
        "function %f(i64) -> i64 {\nblock0(v0: i64):\n    return v0\n}\n",
    )
    .unwrap()
}

struct Recorder {
    name: &'static str,
    log: Rc<RefCell<Vec<&'static str>>>,
}

impl Pass for Recorder {
    fn name(&self) -> &'static str {
        self.name
    }

    fn run(&mut self, _func: &mut Function) {
        self.log.borrow_mut().push(self.name);
    }
}

struct Breaker;

impl Pass for Breaker {
    fn name(&self) -> &'static str {
        "breaker"
    }

    fn run(&mut self, func: &mut Function) {
        let bogus = Value::from_index(999);
        let entry = func.entry_block();
        func.push_inst(
            entry,
            InstData::Binary {
                op: BinOp::Iadd,
                args: [bogus, bogus],
            },
        );
    }
}

#[test]
fn null_pass_leaves_ir_unchanged() {
    let mut func = identity();
    let before = func.to_string();
    PassManager::default_pipeline().run(&mut func).unwrap();
    assert_eq!(func.to_string(), before);
}

#[test]
fn passes_run_in_insertion_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let recorder = |name| Recorder {
        name,
        log: Rc::clone(&log),
    };
    let mut manager = PassManager::new();
    manager
        .add(recorder("first"))
        .add(NullPass)
        .add(recorder("second"));
    manager.run(&mut identity()).unwrap();
    assert_eq!(*log.borrow(), ["first", "second"]);
}

#[test]
fn invalid_ir_is_blamed_on_the_pass_that_produced_it() {
    let mut manager = PassManager::new();
    manager
        .add(NullPass)
        .add(Breaker)
        .verify_between_passes(true);
    let err = manager.run(&mut identity()).unwrap_err();
    assert_eq!(err.after, "breaker");
    assert_eq!(
        err.to_string(),
        "function failed verification after `breaker`"
    );
}

#[test]
fn invalid_input_is_reported_before_any_pass_runs() {
    let mut func = Function::new("bad", Signature::default());
    let entry = func.entry_block();
    func.set_terminator(
        entry,
        Terminator::Return {
            values: vec![Value::from_index(5)],
        },
    );

    let mut manager = PassManager::new();
    manager.add(NullPass).verify_between_passes(true);
    assert_eq!(manager.run(&mut func).unwrap_err().after, "input");
}

#[test]
fn verification_can_be_disabled() {
    let mut manager = PassManager::new();
    manager.add(Breaker).verify_between_passes(false);
    assert!(manager.run(&mut identity()).is_ok());
}

struct MultiplyByZero;

impl sbe::sopt::Rewrite for MultiplyByZero {
    fn priority(&self) -> u32 {
        1
    }

    fn accepts(&self, expr: &sbe::sopt::Expr) -> bool {
        matches!(
            expr,
            sbe::sopt::Expr::Binary {
                op: BinOp::Imul,
                ..
            }
        )
    }

    fn rewrite(
        &self,
        graph: &sbe::sopt::EGraph,
        expr: &sbe::sopt::Expr,
    ) -> Option<sbe::sopt::Expr> {
        let sbe::sopt::Expr::Binary { ty, args, .. } = *expr else {
            return None;
        };
        args.iter()
            .any(|&arg| graph.constant(arg) == Some(0))
            .then_some(sbe::sopt::Expr::Iconst { ty, value: 0 })
    }
}

#[test]
fn egraph_rules_cooperate_and_elaboration_is_idempotent() {
    let mut func = parse_function(
        "rewrite",
        "
function %rewrite(i64) -> i64 {
block0(v0: i64):
    v1 = iconst i64 0
    v2 = imul v0, v1
    v3 = iconst i64 7
    v4 = iadd v2, v3
    return v4
}
",
    )
    .unwrap();
    let mut optimizer = sbe::sopt::EGraphOptimizer::default();
    optimizer.add(MultiplyByZero);
    let mut manager = PassManager::new();
    manager.add(optimizer).verify_between_passes(true);
    manager.run(&mut func).unwrap();
    let output = func.to_string();
    assert!(!output.contains("imul"));
    assert!(!output.contains("iadd"));
    assert!(output.contains("iconst i64 7"));
    manager.run(&mut func).unwrap();
    assert_eq!(func.to_string(), output);
}
