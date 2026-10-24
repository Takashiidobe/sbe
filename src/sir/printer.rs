use std::fmt::{self, Display, Formatter, Write};

use super::{Block, FuncRef, Function, InstData, Signature, Terminator, Value};

fn write_values(f: &mut Formatter<'_>, values: &[Value]) -> fmt::Result {
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{value}")?;
    }
    Ok(())
}

fn write_target(f: &mut Formatter<'_>, dest: Block, args: &[Value]) -> fmt::Result {
    write!(f, "{dest}")?;
    if !args.is_empty() {
        f.write_char('(')?;
        write_values(f, args)?;
        f.write_char(')')?;
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

fn write_signature(f: &mut Formatter<'_>, name: &str, signature: &Signature) -> fmt::Result {
    write!(f, "%{name}(")?;
    write_types(f, &signature.params)?;
    f.write_char(')')?;
    if !signature.returns.is_empty() {
        f.write_str(" -> ")?;
        write_types(f, &signature.returns)?;
    }
    Ok(())
}

impl Display for Function {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("function ")?;
        write_signature(f, self.name(), self.signature())?;
        f.write_str(" {\n")?;

        for (i, callee) in self.callees().iter().enumerate() {
            write!(f, "    {} = ", FuncRef::from_index(i))?;
            write_signature(f, &callee.name, &callee.signature)?;
            f.write_char('\n')?;
        }
        if !self.callees().is_empty() {
            f.write_char('\n')?;
        }

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
                f.write_str("    ")?;
                if let Some(result) = self.inst_result(inst) {
                    write!(f, "{result} = ")?;
                }
                match self.inst(inst) {
                    InstData::Iconst { ty, value } => writeln!(f, "iconst {ty} {value}")?,
                    InstData::Convert { op, ty, arg } => {
                        writeln!(f, "{} {ty} {arg}", op.mnemonic())?
                    }
                    InstData::Icmp { cc, args: [a, b] } => {
                        writeln!(f, "icmp {} {a}, {b}", cc.mnemonic())?
                    }
                    InstData::Select { args: [c, a, b] } => writeln!(f, "select {c}, {a}, {b}")?,
                    InstData::Unary { op, arg } => writeln!(f, "{} {arg}", op.mnemonic())?,
                    InstData::Binary { op, args: [a, b] } => {
                        writeln!(f, "{} {a}, {b}", op.mnemonic())?
                    }
                    InstData::Call { callee, args } => {
                        write!(f, "call {callee}(")?;
                        write_values(f, args)?;
                        writeln!(f, ")")?
                    }
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
                    f.write_str("    jump ")?;
                    write_target(f, *dest, args)?;
                    f.write_char('\n')?;
                }
                Some(Terminator::Brif {
                    cond,
                    then_dest,
                    then_args,
                    else_dest,
                    else_args,
                }) => {
                    write!(f, "    brif {cond}, ")?;
                    write_target(f, *then_dest, then_args)?;
                    f.write_str(", ")?;
                    write_target(f, *else_dest, else_args)?;
                    f.write_char('\n')?;
                }
                None => {}
            }
        }

        f.write_str("}\n")
    }
}
