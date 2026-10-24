use object::{Object, ObjectSection, ObjectSymbol, SymbolKind, SymbolScope};
use sbe::sir::parse_function;
use sbe::x86_64::compile;

#[test]
fn object_exports_function_symbol_with_code() {
    let func = parse_function(
        "object",
        "function %add(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = iadd v0, v1\n    return v2\n}\n",
    )
    .unwrap();
    let compiled = compile(&func).unwrap();
    let bytes = compiled.to_object().unwrap();

    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.architecture(), object::Architecture::X86_64);
    assert_eq!(file.format(), object::BinaryFormat::Elf);

    let symbol = file.symbol_by_name("add").expect("symbol `add`");
    assert_eq!(symbol.kind(), SymbolKind::Text);
    assert_eq!(symbol.scope(), SymbolScope::Dynamic);
    assert_eq!(symbol.size(), compiled.code.len() as u64);

    let text = file.section_by_name(".text").expect(".text");
    assert_eq!(text.data().unwrap(), compiled.code.as_slice());
    assert_eq!(symbol.address(), text.address());
    assert!(file.section_by_name(".note.GNU-stack").is_some());
}
