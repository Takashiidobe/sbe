# Architecture

sbe takes functions in its own SSA IR (sir) and produces wasm bytes or x86_64
machine code. Frontends build sir directly; there is no source language here.

```
FunctionBuilder / text parser
        |
       sir  --verify-->  sopt passes  --verify-->  backend
                                                   |-- wasm::compile    -> module bytes + WAT
                                                   `-- x86_64::compile  -> code bytes + asm
```

Both backends call `verify` first, so they only ever see valid IR.

## Modules

- `src/sir/`: IR types, builder, text parser and printer, verifier, control flow
  analysis. See [sir.md](sir.md).
- `src/sopt/`: `Pass` trait and `PassManager`. The only pass is a no-op.
- `src/wasm/`: sir to a wasm module with structured control flow. See
  [wasm.md](wasm.md).
- `src/x86_64/`: sir to virtual-register code, linear-scan allocation, then
  assembly. See [x86_64.md](x86_64.md).
- `tests/`: end-to-end only. See [testing.md](testing.md).

## Ground rules

- Block parameters replace phis. A jump or `brif` passes values to the
  destination's params.
- The entry block's params are the function params, and nothing may jump back to
  it.
- Backends take a verified function and may `expect` on that.
- Every backend decision about integer widths follows one rule: values narrower
  than 64 bits have undefined upper bits until an op needs them. The details are
  in [sir.md](sir.md#integer-widths).

## Where to start

- Adding an instruction: `src/sir/inst.rs`, then the verifier, builder, printer,
  parser (`ast.rs`, `syntax.rs`, `lower.rs`), and both backends. The compiler
  lists every `match` you missed.
- Adding a backend feature: add a filetest first, then regenerate its checks
  with `tools/update_filecheck.py`.
