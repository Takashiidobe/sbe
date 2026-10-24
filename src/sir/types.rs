use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl Type {
    pub fn bits(self) -> u32 {
        match self {
            Type::I8 => 8,
            Type::I16 => 16,
            Type::I32 => 32,
            Type::I64 | Type::F64 => 64,
            Type::F32 => 32,
        }
    }

    pub fn is_int(self) -> bool {
        !self.is_float()
    }

    pub fn is_float(self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    /// `value` reduced to this type's width, as the unsigned bit pattern
    pub fn truncate(self, value: i64) -> u64 {
        match self {
            Type::I64 | Type::F64 => value as u64,
            _ => value as u64 & ((1 << self.bits()) - 1),
        }
    }

    pub fn from_name(text: &str) -> Option<Self> {
        [
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::F32,
            Type::F64,
        ]
        .into_iter()
        .find(|ty| ty.to_string() == text)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = if self.is_float() { 'f' } else { 'i' };
        write!(f, "{prefix}{}", self.bits())
    }
}
