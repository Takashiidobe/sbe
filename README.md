# SBE (small backend)

A small backend with its own IR (sir), optimization (sopt) and emits
x86_64/wasm.

Should be like cranelift, with frontends emitting sir with a textual
representation like clif with basic blocks that take params.

start out with this small language

Frontend languages should emit the IR directly with a builder, and the frontend
is responsible for constructing valid SSA.

For now optimizations are a null pass

The builder api can look like:

For the building of IR, which we can use to test against

```rust
let mut builder = FunctionBuilder::new();

let entry = builder.create_block();
builder.switch_to_block(entry);

let a = builder.iconst(10);
let b = builder.iconst(20);
let sum = builder.iadd(a, b);

builder.ret(sum);

let function = builder.finish();
```

Use filecheck for tests and we should use thiserror + miette for spans and good
diagnostics
