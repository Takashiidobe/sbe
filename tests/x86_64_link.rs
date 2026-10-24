#![cfg(all(target_arch = "x86_64", target_os = "linux"))]

use std::path::Path;
use std::process::Command;

use sbe::sir::parse_function;
use sbe::x86_64::compile;

fn link_and_run(sir: &str, c_main: &str) -> String {
    let func = parse_function("link", sir).unwrap();
    let object = compile(&func).unwrap().to_object().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let object_path = dir.path().join("func.o");
    let main_path = dir.path().join("main.c");
    let exe_path = dir.path().join("prog");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(&main_path, c_main).unwrap();

    run(Command::new("cc")
        .args(["-o"])
        .arg(&exe_path)
        .arg(&main_path)
        .arg(&object_path));
    stdout_of(&exe_path)
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected linker output:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout_of(exe: &Path) -> String {
    let output = Command::new(exe).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn c_harness_calls_add() {
    let out = link_and_run(
        "function %add(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = iadd v0, v1\n    return v2\n}\n",
        r#"#include <stdio.h>
extern long add(long, long);
int main(void) {
    printf("%ld %ld\n", add(2, 3), add(-7, 3));
    return 0;
}
"#,
    );
    assert_eq!(out, "5 -4\n");
}

#[test]
fn c_harness_calls_function_with_wide_constants() {
    let out = link_and_run(
        "function %wide() -> i64 {\nblock0:\n    v0 = iconst i64 10\n    v1 = iconst i64 -20\n    v2 = iconst i64 1099511627776\n    v3 = iadd v0, v1\n    v4 = iadd v3, v2\n    return v4\n}\n",
        r#"#include <stdio.h>
extern long wide(void);
int main(void) {
    printf("%ld\n", wide());
    return 0;
}
"#,
    );
    assert_eq!(out, "1099511627766\n");
}
