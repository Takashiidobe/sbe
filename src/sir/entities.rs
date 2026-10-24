macro_rules! entity {
    ($name:ident) => {
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
    };
}

entity!(Value);
entity!(Block);
entity!(Inst);
