use std::fmt::{self, Display, Formatter, Write};

use super::{Function, InstData, Terminator, Value};

fn write_values(f: &mut Formatter<'_>, values: &[Value]) -> fmt::Result {
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{value}")?;
    }
    Ok(())
}

fn write_types<T: Display>(f: &mut Formatter<'_>, types: &[T]) -> fmt::Result {
    for (i, ty) in types.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{ty}")?;
    }
    Ok(())
}

impl Display for Function {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let signature = self.signature();
        write!(f, "function %{}(", self.name())?;
        write_types(f, &signature.params)?;
        f.write_char(')')?;
        if !signature.returns.is_empty() {
            f.write_str(" -> ")?;
            write_types(f, &signature.returns)?;
        }
        f.write_str(" {\n")?;

        for (i, block) in self.blocks().enumerate() {
            if i > 0 {
                f.write_char('\n')?;
            }
            let data = self.block(block);
            write!(f, "{block}")?;
            if !data.params().is_empty() {
                f.write_char('(')?;
                for (j, &param) in data.params().iter().enumerate() {
                    if j > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{param}: {}", self.value_type(param))?;
                }
                f.write_char(')')?;
            }
            f.write_str(":\n")?;

            for &inst in data.insts() {
                write!(f, "    {} = ", self.inst_result(inst))?;
                match self.inst(inst) {
                    InstData::Iconst { value } => writeln!(f, "iconst {value}")?,
                    InstData::Iadd { args: [a, b] } => writeln!(f, "iadd {a}, {b}")?,
                }
            }

            match data.terminator() {
                Some(Terminator::Return { values }) => {
                    f.write_str("    return")?;
                    if !values.is_empty() {
                        f.write_char(' ')?;
                        write_values(f, values)?;
                    }
                    f.write_char('\n')?;
                }
                Some(Terminator::Jump { dest, args }) => {
                    write!(f, "    jump {dest}")?;
                    if !args.is_empty() {
                        f.write_char('(')?;
                        write_values(f, args)?;
                        f.write_char(')')?;
                    }
                    f.write_char('\n')?;
                }
                None => {}
            }
        }

        f.write_str("}\n")
    }
}
