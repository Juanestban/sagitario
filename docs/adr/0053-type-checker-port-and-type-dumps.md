# 0053. The type checker port and type dumps

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0052](0052-resolver-port-and-resolution-dumps.md)

## Context

F5 increment 5 ports the type checker: about 8 700 lines of Rust in 14 files, twice the resolver
and the loader together. Besides types, it evaluates constants, checks the exhaustiveness of
`match`, analyses flow, infers `Send` and checks FFI, inheritance and interfaces. Its result, the
`TypedModule`, has 25 tables. Increment 5 settles Q250–Q259.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q250 | Split | Two increments. **5a, declarations**: the definitions of structs, enums, classes and interfaces, generics and recursive types, `@repr(C)`, the signatures of functions and of `extern` functions, constants, C symbols, inheritance, interfaces and `main`. **5b, bodies**: expressions, patterns, closures, collections, exhaustiveness, which methods modify `self`, `Send` and the flow analysis. |
| Q251 | Dumps | `sag check --dump=declarations` stops after the declarations, and `--dump=types` (5b) runs the whole check. The stages split exactly where `check_main` ends: `sagitario_types::check_until(..., Stage::Declarations \| Stage::Bodies)`, which `check` calls with `Bodies`. |
| Q252 | Format | Sections in a fixed order, maps sorted by key, as in the earlier dumps. Types are written as in messages, with the identity of program types and parameters: `Point#struct2<i32>`, `T#fn3.0`, `Self#interface1.self`. The `#` belongs to the dump only, like `path #12` in the resolver's; it is not syntax, and messages do not change. |
| Q253 | `Type` in Sagitario | A class holding a `TypeKind` enum, as `Expr` in the parser; `Box<Type>` becomes `Type`. The four `StructRef`, `EnumRef`, `ClassRef` and `InterfaceRef` are one `NamedRef`. Structural equality (ignoring names, as Rust's `PartialEq`) is `types::same_type`, since Sagitario has no `==` on enums with data. |
| Q254 | 128-bit integers | Rust computes constants in `i128`. Every value that fits an integer type lies between `i64::MIN` and `u64::MAX`, so `sagc` keeps a sign and a `u64` magnitude (`Wide`), with overflow checks written out, and a result beyond that fits no type, as Rust's range check finds. No `i128` in the language: C11 has no portable 128-bit integers. |
| Q255 | Floats | Constants are computed in `f64` in both compilers and written with Sagitario's interpolation, which already follows Rust's `Display` (ADR-0020). The corpus has limit cases. |
| Q256 | Target | `sagc`'s host has 64-bit pointers, a 32-bit `long` on Windows (told apart as `std::path` does) and a signed `char`. Only a Linux ARM host would differ; real detection and `--target` come with the driver (increment 7). |
| Q257 | Files | `compiler/src/types.sag` holds the types, the definitions and the `Checker` with its core; `types/typed.sag` the `TypedModule` and its queries; `types/{classes, collections, consts, ffi, generic, inheritance, interfaces, send}.sag` one area each, as functions taking the checker, since a Sagitario class's methods are all in its body. |
| Q258 | Corpus | `tests/types/`: constants at the limits of every integer type, float constants and casts, classes and vtables, interfaces, generics and `Send`, FFI signatures and symbols, and the errors of declarations. With every `.sag` file of the repository, in both diagnostic formats. |
| Q260 | The `types` dump (5b) | `sag check --dump=types` runs the whole check, flow included, with the same dumper, which already has every table: the type of each expression and local, the type arguments of each call, built-in methods, method calls, `zod` calls, fields called as functions, iterators and ranges. |
| Q261 | Dumps in the test | The test `selfhost` compares `types` instead of `declarations`, which `types` contains, tables and diagnostics; `--dump=declarations` stays for debugging. |
| Q262 | Files of 5b | Expressions go to `compiler/src/types/exprs.sag` as functions taking the checker; `patterns`, `exhaustive`, `closures` and `flow` have their own files (`flow` is a class of its own, as in Rust). The `text` module of Rust is `types/strings.sag`, since `types::text` writes a type. |
| Q263 | Integers in bodies | Literals use the checks of `Wide`. The constructors of exhaustiveness only need equality, so `Ctor::Int` holds the normalized decimal text, with Rust's limit: no constructor for a literal beyond `i128::MAX`. |
| Q264 | `{:?}` in messages | Exhaustiveness writes `char` and `String` constructors with Rust's `Debug`, which escapes by a Unicode table of printable characters. Those never reach a message (the witnesses shown hold variants, `bool` and `_` only), so `sagc` uses the quoting of the dumps, and Rust does not change. |
| Q265 | Rust changes of 5b | `Dump::Types` in the driver and the CLI; no result of the bodies depends on the order of a `HashMap`. |
| Q266 | Corpus of 5b | `tests/types/bodies.sag` and `tests/types/flow.sag`: local and generic inference, closures and captures, collections, exhaustiveness and unreachable arms, methods that modify `self`, definite assignment, `const` assigned twice, unreachable code, the binding warnings and `thread::spawn` with `Send`. |
| Q259 | Rust changes | The stages (Q251), the dump, and the same rule as Q249 for results that depended on something other than the program. Two fixes: the classes of `std::sync` came from iterating a `HashMap` and are now sorted by id, and a failed `include_str` reported the operating system's message, which differs between platforms and C runtimes; it now names the reason by kind (`the file does not exist`, `permission denied`, `the file is not UTF-8`, `the file cannot be read`). |

## Alternatives considered

- **One increment for the whole checker:** a difference in the declarations would show up mixed
  with thousands of lines of body checking, and nothing would be comparable until the end.
- **Dumping types by name only:** two types with one name in different modules, or a function's
  `T` and its struct's `T`, would look the same, and confusing them would go unnoticed.
- **Adding `i128` to the language:** a large feature for one internal use, without a portable C
  type behind it.

## Consequences

- `sagitario_ast::dump::quote` is public, so the type dump quotes text like the other dumps.
- With 5b, `sagc` checks the whole front end (lexer to types) against the Rust compiler on every
  `.sag` file of the repository.
- In `sagc`, a flow state is copied explicitly wherever Rust clones one: lists are references.
- `Resolution` in `sagc` gains the queries the checker asks: `initializer`, `is_method`,
  `prelude_type`, `is_std_source` and `is_runtime_extern`.
