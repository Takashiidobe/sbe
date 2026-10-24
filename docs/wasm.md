# wasm backend

`wasm::compile(&func)` returns the module bytes and its WAT text. Every sir
value gets its own wasm local; params are the first locals.

## Modules and calls

`compile_module(&module)` puts imports first in the function index space, under
module name `env`, then the defined functions, each exported by name. `compile`
on a single function that calls others imports every distinct callee.

## Control flow

Reducible functions use structured control flow built from the dominator tree
(Ramsey's "Beyond Relooper"): a `block` per merge point, a `loop` per loop
header, and `br` to the right depth. Irreducible functions fall back to a loop
around a `br_table` on a block-index local.

## Integer widths

`i8`, `i16` and `i32` are all wasm `i32` locals. `get_extended` applies
`extend8_s`/`extend16_s` or a mask for `i8`/`i16`. `i32` needs no fixup because
it is native.

## Lessons

- wasm `rem_s` returns 0 for `MIN % -1`, but sir traps. `srem` on `i64` checks
  explicitly.
- `i32` signed division runs in `i64` and wraps the result. That makes `i32`
  behave like `i8` and `i16` and matches x86_64.
- Masks on shift counts are only needed for `i8` and `i16`; wasm masks `i32` and
  `i64` itself.
- The structured tail looks reachable to the validator even when every path
  returns, so the function body ends with `unreachable`.
- Block params are assigned with all args pushed first and then set in reverse,
  so swaps work without temporaries.
