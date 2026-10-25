# 0054. The lowering and C generation port

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0053](0053-type-checker-port-and-type-dumps.md)

## Context

F5 increment 6 ports the back end: the lowering to SIR (`sagitario_sir`, about 6 000 lines of
Rust in 7 files) and the C generator (`sagitario_codegen_c`, about 1 600). Together they
monomorphize generics, build vtables, itables and closure environments, insert reference counting
and write strict C11. Increment 6 settles Q267–Q274.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q267 | Scope | One increment for `sir` and `codegen_c`, compared through the generated C. The C is close to a one-to-one transcription of the SIR, so a difference in the SIR shows up in the C near its cause; a SIR dumper would cost about 400 lines in each language to repeat what the C already shows. |
| Q268 | Dump | `sag check --dump=c` runs what `build` runs up to generating C, without calling a C compiler. It prints the program's translation unit and, with FFI, the line `/* ---- sag_ffi.c ---- */` followed by the FFI unit. Front-end errors stop it as they stop `build`; diagnostics of the lowering (generic code instantiating itself endlessly) and internal compiler errors (exit code 101) are reported as in the driver. The runtime is a fixed file and is not dumped. |
| Q269 | Float constants | The C generator wrote the `f64` of constants with Rust's `Debug`, which uses an exponent outside [1e-4, 1e16). It now uses `Display`, the shortest round trip without an exponent, which Sagitario's interpolation already produces (ADR-0020). The C reads the same `double`, so behavior does not change; only the text of very large or very small constants does. |
| Q270 | SIR in Sagitario | `Stmt`, `Rvalue`, `Operand`, `Place`, `Projection` and `Const` are enums. Nested statements (`If`, `Loop`, the `for` forms) hold `List<Stmt>`, which is not recursion by value since a list is a reference. Types are the `Type` of `types.sag`. A `Place` is copied whenever it is projected, since its projections are a list. |
| Q271 | Files | `compiler/src/sir.sag` holds the SIR, the registry of instances and the core of the lowering; the areas are `compiler/src/sir/{closures, collections, interfaces, matching, mono, strings}.sag`, as functions taking the lowering. The `text` module of Rust is `sir/strings.sag`, as in the type checker (Q262). The generator is a class in `compiler/src/codegen_c.sag`, as in Rust. |
| Q272 | Integers | `Const::Int` holds only values already checked, at most `u64::MAX` in magnitude, so `sagc` keeps a sign and a `u64` magnitude, as `Wide` does for constants (Q254). Registries keyed by types in Rust (`HashMap<Type, _>`) are maps keyed by the text of `type_key`, which writes the identity of a type. Rust's closures that buffer statements become explicit marks (`open_buffer`, `close_buffer`), and a lowering error is a `String`. |
| Q273 | Corpus | `tests/codegen/`: floats at their extremes and `-0.0`, integers at their limits, strings longer than 4 095 bytes (static arrays), octal escapes and trigraphs, characters outside ASCII (`literals.sag`), and the combinations the lowering handles in separate places (`lowering.sag`). FFI and threads are covered by `tests/run-pass/ffi.sag`, `threads.sag` and `sync.sag`. Every `.sag` file of the repository must generate the same C. |
| Q274 | Rust changes | The dump (`Dump::C`) and Q269. No output of the lowering or the generator depends on the order of a `HashMap`: they only look keys up. |

## Alternatives considered

- **Two increments, SIR then C (as with the types):** a SIR dumper in both compilers for a
  representation the C already mirrors.
- **Porting the exponent rules of Rust's `Debug` for floats:** more code in `sagc` for a format
  nothing else in Sagitario uses.
- **Dumping the runtime with the program:** it is the same file for every program.

## Consequences

- With increment 6, `sagc` generates the C of every `.sag` file of the repository exactly as the
  Rust compiler does; increment 7 ports the driver that compiles it.
- The corpus found a runtime bug: `sag_f64_to_f32` turned values just beyond `FLT_MAX` into
  infinity, and the Rust compiler, folding a constant `3.4028235e38 as f32`, gave `FLT_MAX`. The
  spec rounds to nearest (docs/spec/02-types.md#conversions), so values up to halfway to 2^128 now
  round to `FLT_MAX`, which also keeps the C conversion in range. `tests/run-pass/arithmetic_and_casts.sag`
  covers it.
- `SourceMap` gains `len`, `Resolution` gains `in_prelude` and `in_library`, and the
  `TypedModule` of `sagc` gains `interface_path`, `class_parent` and `local_type`.
