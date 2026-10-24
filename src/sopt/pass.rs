use crate::sir::Function;

pub trait Pass {
    fn name(&self) -> &'static str;

    fn run(&mut self, func: &mut Function);
}

pub struct NullPass;

impl Pass for NullPass {
    fn name(&self) -> &'static str {
        "null"
    }

    fn run(&mut self, _func: &mut Function) {}
}
