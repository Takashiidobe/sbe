use std::fmt;

macro_rules! entity {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            pub fn from_index(index: usize) -> Self {
                Self(u32::try_from(index).expect("entity index overflows u32"))
            }

            pub fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

entity!(Value, "v");
entity!(Block, "block");
entity!(Inst, "inst");
