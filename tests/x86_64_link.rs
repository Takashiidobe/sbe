#![cfg(all(target_arch = "x86_64", target_os = "linux"))]

mod common;

use std::path::Path;
use std::process::Command;

use sbe::sir::parse_function;
use sbe::sir::parse_module;
use sbe::x86_64::{compile, compile_module, module_object};

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

#[test]
fn module_calls_c_imports_and_sibling_functions() {
    let module = parse_module("calls", include_str!("programs/calls.sir")).unwrap();
    let object = module_object(&compile_module(&module).unwrap()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let object_path = dir.path().join("module.o");
    let main_path = dir.path().join("main.c");
    let exe_path = dir.path().join("prog");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(
        &main_path,
        r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
long scale(long x) {
    if ((uintptr_t)__builtin_frame_address(0) % 16 != 0) abort();
    return x * 2;
}
void note(long x) { printf("note %ld\n", x); }
extern long fact(long);
extern long run(long);
extern long pair(long, long);
extern long sum5(long, long, long, long, long);
int main(void) {
    printf("%ld %ld %ld\n", fact(10), run(3), pair(5, 7));
    printf("%ld\n", sum5(1, 2, 3, 4, 5));
    return 0;
}
"#,
    )
    .unwrap();

    run(Command::new("cc")
        .args(["-O0", "-fno-omit-frame-pointer", "-o"])
        .arg(&exe_path)
        .arg(&main_path)
        .arg(&object_path));
    assert_eq!(stdout_of(&exe_path), "note 6\n3628800 723 22\n17\n");
}

#[test]
fn module_memory_against_c_harness() {
    let module = parse_module("memory", include_str!("programs/memory.sir")).unwrap();
    let object = module_object(&compile_module(&module).unwrap()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let object_path = dir.path().join("module.o");
    let main_path = dir.path().join("main.c");
    let exe_path = dir.path().join("prog");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(
        &main_path,
        r#"#include <stdint.h>
#include <stdio.h>
extern long triple(long);
extern long widths(long);
extern long squares(long);
extern long rec(long);
extern long read_i32s(const int32_t *);
extern void write_byte(unsigned char *, int);
int main(void) {
    printf("%ld %ld\n", triple(7), widths(0x1122334455667788L));
    printf("%ld %ld\n", squares(8), rec(100));
    int32_t pair[2] = {1, -2};
    unsigned char byte = 0;
    write_byte(&byte, 0x1ff);
    printf("%ld %d\n", read_i32s(pair), byte);
    return 0;
}
"#,
    )
    .unwrap();
    run(Command::new("cc")
        .args(["-O0", "-fno-omit-frame-pointer", "-o"])
        .arg(&exe_path)
        .arg(&main_path)
        .arg(&object_path));
    let expected = format!(
        "{} {}\n{} {}\n-1 255\n",
        common::memory_triple(7),
        common::memory_widths(0x1122334455667788),
        common::memory_squares(8),
        common::memory_rec(100),
    );
    assert_eq!(stdout_of(&exe_path), expected);
}

#[test]
fn module_switch_against_c_harness() {
    let module = parse_module("switch", include_str!("programs/switch.sir")).unwrap();
    let object = module_object(&compile_module(&module).unwrap()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let object_path = dir.path().join("module.o");
    let main_path = dir.path().join("main.c");
    let exe_path = dir.path().join("prog");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(
        &main_path,
        r#"#include <stdio.h>
extern long classify(long);
extern long by_byte(long);
extern long checked_div(long, long);
extern long ping(long, long);
int main(void) {
    long xs[] = {0, 1, -1, 5, 2, 3, 100, -100};
    for (int i = 0; i < 8; i++) printf("%ld ", classify(xs[i]));
    printf("\n");
    long bs[] = {0, 1, -1, 255, 257, -255, 256, 5};
    for (int i = 0; i < 8; i++) printf("%ld ", by_byte(bs[i]));
    printf("\n");
    printf("%ld %ld %ld %ld %ld\n", checked_div(-9, 2), ping(0, 7), ping(1, 4), ping(2, 4), ping(1, -3));
    return 0;
}
"#,
    )
    .unwrap();
    run(Command::new("cc")
        .args(["-O0", "-fno-omit-frame-pointer", "-o"])
        .arg(&exe_path)
        .arg(&main_path)
        .arg(&object_path));

    let row = |values: Vec<i64>| values.iter().map(|v| format!("{v} ")).collect::<String>();
    let expected = format!(
        "{}\n{}\n-4 7 4 -1 0\n",
        row([0, 1, -1, 5, 2, 3, 100, -100]
            .map(common::switch_classify)
            .to_vec()),
        row(common::SWITCH_BYTE_SAMPLES
            .map(common::switch_by_byte)
            .to_vec()),
    );
    assert_eq!(stdout_of(&exe_path), expected);
}
