use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
}

impl Type {
    pub fn bits(self) -> u32 {
        match self {
            Type::I8 => 8,
            Type::I16 => 16,
            Type::I32 => 32,
            Type::I64 => 64,
        }
    }

    /// `value` reduced to this type's width, as the unsigned bit pattern
    pub fn truncate(self, value: i64) -> u64 {
        match self {
            Type::I64 => value as u64,
            _ => value as u64 & ((1 << self.bits()) - 1),
        }
    }

    pub fn from_name(text: &str) -> Option<Self> {
        [Type::I8, Type::I16, Type::I32, Type::I64]
            .into_iter()
            .find(|ty| ty.to_string() == text)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "i{}", self.bits())
    }
}
