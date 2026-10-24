use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    I64,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I64 => f.write_str("i64"),
        }
    }
}
