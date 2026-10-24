use std::collections::HashMap;

use miette::Diagnostic;
use thiserror::Error;

use super::{Function, Signature, VerifyErrors, verify};

/// a function defined elsewhere, known only by name and signature
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    pub signature: Signature,
}

/// functions and imports share one namespace. calls name their callee; [`verify_module`] checks
/// that each name resolves to a function or an import with the declared signature.
#[derive(Clone, Debug, Default)]
pub struct Module {
    functions: Vec<Function>,
    imports: Vec<Import>,
}

impl Module {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_function(&mut self, func: Function) {
        self.functions.push(func);
    }

    pub fn declare_import(&mut self, name: impl Into<String>, signature: Signature) {
        self.imports.push(Import {
            name: name.into(),
            signature,
        });
    }

    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    pub fn imports(&self) -> &[Import] {
        &self.imports
    }

    pub fn signature_of(&self, name: &str) -> Option<&Signature> {
        let defined = self.functions.iter().find(|f| f.name() == name);
        let imported = self.imports.iter().find(|i| i.name == name);
        defined
            .map(Function::signature)
            .or(imported.map(|i| &i.signature))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
pub enum ModuleError {
    #[error("`%{0}` is defined more than once")]
    #[diagnostic(code(sir::module::duplicate_name))]
    DuplicateName(String),

    #[error("%{caller} calls `%{callee}`, which is neither defined nor imported")]
    #[diagnostic(code(sir::module::unresolved_callee))]
    UnresolvedCallee { caller: String, callee: String },

    #[error("%{caller} declares `%{callee}` with a signature that differs from its definition")]
    #[diagnostic(code(sir::module::signature_mismatch))]
    SignatureMismatch { caller: String, callee: String },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(#[from] VerifyErrors),
}

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
#[error("module failed verification")]
pub struct ModuleErrors {
    #[related]
    pub errors: Vec<ModuleError>,
}

pub fn verify_module(module: &Module) -> Result<(), ModuleErrors> {
    let mut errors = Vec::new();

    let mut seen: HashMap<&str, usize> = HashMap::new();
    let names = module
        .functions
        .iter()
        .map(Function::name)
        .chain(module.imports.iter().map(|i| i.name.as_str()));
    for name in names {
        let count = seen.entry(name).or_default();
        *count += 1;
        if *count == 2 {
            errors.push(ModuleError::DuplicateName(name.to_owned()));
        }
    }

    for func in &module.functions {
        if let Err(invalid) = verify(func) {
            errors.push(invalid.into());
        }
        for callee in func.callees() {
            match module.signature_of(&callee.name) {
                None => errors.push(ModuleError::UnresolvedCallee {
                    caller: func.name().to_owned(),
                    callee: callee.name.clone(),
                }),
                Some(signature) if *signature != callee.signature => {
                    errors.push(ModuleError::SignatureMismatch {
                        caller: func.name().to_owned(),
                        callee: callee.name.clone(),
                    })
                }
                Some(_) => {}
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ModuleErrors { errors })
    }
}
