use miette::Diagnostic;
use thiserror::Error;

use super::{EGraphOptimizer, Pass};
use crate::sir::{Function, VerifyErrors, verify};

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
#[error("function failed verification after `{after}`")]
#[diagnostic(code(sopt::invalid_ir))]
pub struct PassError {
    pub after: String,
    #[diagnostic_source]
    pub errors: VerifyErrors,
}

pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
    verify_between: bool,
}

impl PassManager {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            verify_between: cfg!(debug_assertions),
        }
    }

    pub fn default_pipeline() -> Self {
        let mut manager = Self::new();
        manager.add(EGraphOptimizer::default());
        manager
    }

    pub fn add(&mut self, pass: impl Pass + 'static) -> &mut Self {
        self.passes.push(Box::new(pass));
        self
    }

    pub fn verify_between_passes(&mut self, enabled: bool) -> &mut Self {
        self.verify_between = enabled;
        self
    }

    pub fn run(&mut self, func: &mut Function) -> Result<(), PassError> {
        if self.verify_between {
            check(func, "input")?;
        }
        for pass in &mut self.passes {
            pass.run(func);
            if self.verify_between {
                check(func, pass.name())?;
            }
        }
        Ok(())
    }
}

impl Default for PassManager {
    fn default() -> Self {
        Self::new()
    }
}

fn check(func: &Function, after: &str) -> Result<(), PassError> {
    verify(func).map_err(|errors| PassError {
        after: after.to_owned(),
        errors,
    })
}
