use std::collections::HashMap;

use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, RelocationEncoding, RelocationFlags, RelocationKind,
    SectionKind, SymbolFlags, SymbolKind, SymbolScope,
};

use super::emit::Compiled;
use super::error::CompileError;

impl Compiled {
    /// an ELF relocatable exporting the function as a global symbol named after it
    pub fn to_object(&self) -> Result<Vec<u8>, CompileError> {
        module_object(std::slice::from_ref(self))
    }
}

/// an ELF relocatable exporting every function as a global symbol. calls are `R_X86_64_PLT32`
/// relocations against the callee's name, so they bind to a sibling function or to an undefined
/// symbol the linker supplies.
pub fn module_object(functions: &[Compiled]) -> Result<Vec<u8>, CompileError> {
    let mut obj = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
    let text = obj.section_id(StandardSection::Text);

    let mut symbols: HashMap<&str, SymbolId> = HashMap::new();
    let mut offsets = Vec::new();
    for func in functions {
        let offset = obj.append_section_data(text, &func.code, 16);
        offsets.push(offset);
        let id = obj.add_symbol(Symbol {
            name: func.name.clone().into_bytes(),
            value: offset,
            size: func.code.len() as u64,
            kind: SymbolKind::Text,
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });
        symbols.insert(&func.name, id);
    }

    for (func, &base) in functions.iter().zip(&offsets) {
        for reloc in &func.relocs {
            let symbol = *symbols.entry(&reloc.symbol).or_insert_with(|| {
                obj.add_symbol(Symbol {
                    name: reloc.symbol.clone().into_bytes(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Unknown,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                })
            });
            obj.add_relocation(
                text,
                Relocation {
                    offset: base + reloc.offset as u64,
                    symbol,
                    addend: -4,
                    flags: RelocationFlags::Generic {
                        kind: RelocationKind::PltRelative,
                        encoding: RelocationEncoding::X86Branch,
                        size: 32,
                    },
                },
            )
            .map_err(|e| CompileError::Object(e.to_string()))?;
        }
    }

    obj.add_section(Vec::new(), b".note.GNU-stack".to_vec(), SectionKind::Other);
    obj.write().map_err(|e| CompileError::Object(e.to_string()))
}
