# 0049. The self-hosting plan, module-level constants, `include_str`, `if let` and `while let`

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0007](0007-mutability-const-var.md) (module-level `const`; `let` only in
  `if let`/`while let`), [ADR-0014](0014-mvp-scope.md) (F5)

## Context

F5 rewrites the compiler in Sagitario until it compiles itself. The Rust compiler (about 26 000
lines in 13 crates) was written to be portable: plain structs, enums, functions and simple
traits. Sagitario has almost everything it uses: enums with data and `match`, `List`, `Map`,
interfaces, generics, closures, `std::fs`, `std::process::Command` and `std::env`. Three
things were missing:

- Module-level constants, which the specification defined but the parser rejected (`E0900`).
- Embedding files at compile time: the driver embeds the C runtime and the standard library's
  sources with Rust's `include_str!`.
- `if let` and `while let`. They are not essential, but a lexer and a parser are full of
  `while let Some(token) = ...`, and a `match` for each one multiplies the code.

The other deferred decisions (derived `Eq`/`Hash`, `Display`, tuples, slices, `pub use`) have
workarounds with structs and hand-written methods. This record settles the plan, Q211–Q219,
and the first increment, Q220–Q225.

## Decision

### The plan

| # | Topic | Decision |
|---|---|---|
| Q211 | Strategy | Port pass by pass, with the module structure of the crates. Each pass is checked with a differential test against Rust's: the same tokens, AST, diagnostics and generated C for every program under `tests/`. |
| Q212 | Where | A Nova package in `compiler/`, building the binary `sagc` during F5; it becomes `sag` when it replaces the Rust compiler. `sag build compiler/src/main.sag` builds it, since `sag` resolves the module tree. |
| Q213 | Bootstrap | Stage 0 is the Rust compiler, stage 1 is `sagc` built by stage 0, stage 2 is `sagc` built by stage 1. The fixed point: stages 1 and 2 generate the same C for the compiler itself. A CI job checks it. |
| Q214 | The Rust compiler | It stays the reference and the bootstrap compiler during F5, and language changes go into both. After F5, a decision record says whether it retires, for example by bootstrapping from committed generated C. |
| Q215 | Prerequisites (increment 1) | Module-level constants, `include_str` and `if let`/`while let`, below. |
| Q216 | Dumps | The Rust compiler gains a hidden `--dump=tokens\|ast\|resolve\|types` with a deterministic text format, which `sagc` reproduces. Generated C is compared through `--emit-c`. |
| Q217 | Scope | Only the compiler; Nova stays in Rust until after F5. |
| Q218 | Performance | Not a gate. The time of stage 1 compiling itself, against Rust's, is measured for information. |
| Q219 | Acceptance | Stage 1 passes every suite on the three systems with the three C compilers, and stage 2 reaches the fixed point. |

Increments: (1) the language prerequisites; (2) dumps, and `span`, `diagnostics` and `lexer`;
(3) `parser`; (4) `resolve`; (5) `types`; (6) `sir` and `codegen_c`; (7) the driver and the
CLI; (8) the bootstrap to the fixed point, the CI job and the acceptance.

### Increment 1

| # | Topic | Decision |
|---|---|---|
| Q220 | Constants | `const NAME: Type = value;` at module level, with `pub` to export it, and the type always written. Types: integers, floats, `bool`, `char` and `String`. They are items: imported with `use`, named in full (`limits::MAX`), private to their module without `pub`. |
| Q221 | Values | The compiler computes them from literals, other constants (a constant cannot need itself), unary `-` and `!`, arithmetic (`+ - * / %`, and `+` on strings), comparisons, `&&`, `\|\|`, `as` between primitive types and `include_str`. A value outside its type, a division by zero or a float that is not finite is an error (`E0334`), as is anything else, such as a call. |
| Q222 | `include_str` | `include_str("path")`, named after Rust's `include_str!`, gives the text of a file as a `String`. It is only valid as (part of) the value of a module-level constant. The path is a string literal relative to the directory of the source file, and the file must be UTF-8. Anything else is `E0335`. |
| Q223 | Long strings | Strings longer than 4095 bytes, the longest literal every C11 compiler accepts (MSVC accepts little more), are generated as static byte arrays. |
| Q224 | `if let`, `while let` | `if let pattern = value { ... } else ...` and `while let pattern = value { ... }`, as in Rust. They mean `match value { pattern => { ... }, _ => else }` and `loop { match value { pattern => { ... }, _ => break } }`, so they follow the rules of `match`: bindings only in the block, any pattern, `else if` and `else if let` chains. |
| Q225 | `let` | A keyword, only used in `if let` and `while let`; bindings are still `const` and `var`. |

## Alternatives considered

- **`embed("path")`:** the name did not say what it does; `include_str` is known from Rust.
- **Constants as inlined expressions instead of computed values:** errors such as overflow
  would appear at run time, and `include_str` would read the file at every use.
- **`let` as a contextual word only after `if` and `while`:** `if let - 1 ...` is ambiguous.
- **Without `if let`/`while let`:** every optional value in the compiler would need a
  `match` with a `_` arm.
- **Porting the compiler in one step:** without passes checked against Rust's, a difference
  would only show at the end, in the generated C.

## Consequences

- `Item::Const`, `ConstId`, `ItemRef::Const`, `Res::Const`; the type checker computes
  `TypedModule::const_values`, and SIR uses them as constants. `Module::location` gives
  `include_str` the source file's path.
- The code generator collects long strings and defines them before the functions.
- Programs naming something `let` must rename it.
