use std::fs;
use std::path::{Path, PathBuf};

use filecheck::{CheckerBuilder, NO_VARIABLES};
use libtest_mimic::{Arguments, Failed, Trial};
use miette::{Diagnostic, NarratableReportHandler};
use sbe::sir::{Function, ParseError, parse_function, verify};
use sbe::sopt::PassManager;
use sbe::{wasm, x86_64};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/filetests");

fn main() {
    let args = Arguments::from_args();
    let trials = collect(Path::new(ROOT))
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(ROOT)
                .unwrap()
                .with_extension("")
                .display()
                .to_string();
            Trial::test(name, move || run(&path))
        })
        .collect();
    libtest_mimic::run(&args, trials).exit();
}

fn collect(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(collect(&path));
        } else if path.extension().is_some_and(|ext| ext == "sir") {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn run(path: &Path) -> Result<(), Failed> {
    let source = fs::read_to_string(path)?;
    let stage = stage_of(&source)?;
    let output = run_stage(&stage, path, &source)?;
    dump_output(path, &output)?;

    let checker = CheckerBuilder::new().text(&source)?.finish();
    if checker.is_empty() {
        return Err("file has no filecheck directives".into());
    }
    let (ok, explanation) = checker.explain(&output, NO_VARIABLES)?;
    if ok {
        Ok(())
    } else {
        Err(format!("{explanation}\n--- output ---\n{output}").into())
    }
}

fn dump_output(path: &Path, output: &str) -> Result<(), Failed> {
    let Some(dir) = std::env::var_os("FILECHECK_DUMP_DIR") else {
        return Ok(());
    };
    let target = Path::new(&dir).join(path.strip_prefix(ROOT)?.with_extension("out"));
    fs::create_dir_all(target.parent().unwrap())?;
    fs::write(target, output)?;
    Ok(())
}

fn stage_of(source: &str) -> Result<String, Failed> {
    source
        .lines()
        .find_map(|line| line.strip_prefix("; test "))
        .map(|stage| stage.trim().to_owned())
        .ok_or_else(|| "missing `; test <stage>` directive".into())
}

fn run_stage(stage: &str, path: &Path, source: &str) -> Result<String, Failed> {
    let name = path.file_name().unwrap().to_string_lossy();
    let parsed = parse_function(&name, source);
    match stage {
        "print" => {
            let func = parsed.map_err(render)?;
            verify(&func).map_err(render)?;
            Ok(func.to_string())
        }
        "opt" => {
            let mut func = parsed.map_err(render)?;
            verify(&func).map_err(render)?;
            PassManager::default_pipeline()
                .verify_between_passes(true)
                .run(&mut func)
                .map_err(render)?;
            Ok(func.to_string())
        }
        "x86_64-vcode" => {
            let func = verified(parsed)?;
            Ok(x86_64::lower(&func).map_err(render)?.to_string())
        }
        "x86_64" => Ok(x86_64::compile(&verified(parsed)?).map_err(render)?.asm),
        "x86_64-hex" => {
            let code = x86_64::compile(&verified(parsed)?).map_err(render)?.code;
            let hex: Vec<_> = code.iter().map(|b| format!("{b:02x}")).collect();
            Ok(format!("{}\n", hex.join(" ")))
        }
        "x86_64-error" => match x86_64::compile(&verified(parsed)?) {
            Ok(_) => Err("expected a compile error".into()),
            Err(err) => Ok(render_string(&err)),
        },
        "wasm" => {
            let compiled = wasm::compile(&verified(parsed)?).map_err(render)?;
            wasmparser::validate(&compiled.code).map_err(|e| format!("invalid wasm: {e}"))?;
            Ok(compiled.wat)
        }
        "wasm-error" => match wasm::compile(&verified(parsed)?) {
            Ok(_) => Err("expected a compile error".into()),
            Err(err) => Ok(render_string(&err)),
        },
        "verify" => {
            let func = parsed.map_err(render)?;
            Ok(match verify(&func) {
                Ok(()) => "ok\n".to_owned(),
                Err(errors) => render_string(&errors),
            })
        }
        "parse-error" => match parsed {
            Ok(_) => Err("expected a parse error".into()),
            Err(err) => Ok(render_string(&err)),
        },
        other => Err(format!("unknown stage `{other}`").into()),
    }
}

fn verified(parsed: Result<Function, ParseError>) -> Result<Function, Failed> {
    let func = parsed.map_err(render)?;
    verify(&func).map_err(render)?;
    Ok(func)
}

fn render(diagnostic: impl Diagnostic) -> Failed {
    render_string(&diagnostic).into()
}

fn render_string(diagnostic: &dyn Diagnostic) -> String {
    let mut out = String::new();
    NarratableReportHandler::new()
        .render_report(&mut out, diagnostic)
        .unwrap();
    out
}
