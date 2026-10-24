use object::write::{Object, StandardSection, Symbol, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
};

use super::emit::Compiled;
use super::error::CompileError;

impl Compiled {
    /// an ELF relocatable exporting the function as a global symbol named after it
    pub fn to_object(&self) -> Result<Vec<u8>, CompileError> {
        let mut obj = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
        let text = obj.section_id(StandardSection::Text);
        let offset = obj.append_section_data(text, &self.code, 16);
        obj.add_symbol(Symbol {
            name: self.name.clone().into_bytes(),
            value: offset,
            size: self.code.len() as u64,
            kind: SymbolKind::Text,
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });
        obj.add_section(Vec::new(), b".note.GNU-stack".to_vec(), SectionKind::Other);
        obj.write().map_err(|e| CompileError::Object(e.to_string()))
    }
}
