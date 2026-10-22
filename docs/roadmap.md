# Roadmap

Rationale: [ADR-0014](adr/0014-mvp-scope.md). **The MVP is F0 + F1 + F2, desktop only.**

## F0: Infrastructure (MVP)

- Valid virtual workspace (`crates/*`), remove the root `src/main.rs`, shared lint config.
- `sag` CLI skeleton (`build`, `run`, `check`).
- End-to-end test harness and CI matrix (Windows, Linux, macOS + gcc job).

**Done when:** CI is green on all three OSes with the harness running an empty suite.

## F1: Procedural core (MVP)

Lexer, parser, AST, diagnostics with line/column, multi-file module resolution, type checking
with local inference, SIR, C backend. Language features: primitives, `const`/`var`,
functions, `return`, `if`/`else`, `while`, `loop`, `for` over ranges, `break`/`continue`,
`struct` with methods, enums with data, exhaustive `match`, `Option`/`Result` with `?`, basic
UTF-8 `String`, arrays, `extern "C"`/`unsafe`, `panic`, printing.

Built as a vertical slice first, then widened feature by feature through every phase:

1. Vertical slice: `pub fn main()`, prelude `println` with string literals, unit functions.
2. `const`/`var`, operators with overflow checks, `if`/`else`/`while`/`loop`/`for` over ranges,
   functions with parameters and return values, basic template-string interpolation.
3. Diagnostic quality and the first broad `compile-fail` suites.
4. Structs with methods and static functions; fixed arrays.
5. Enums, exhaustive `match`, `Option`/`Result` and `?`.
6. UTF-8 `String`, full template strings, `panic` and `run-panic` suites.
7. Multi-file modules, `use` and visibility.
8. FFI: `extern "C"`, `unsafe`, `@export`, `@repr(C)`.
9. The reference programs below.

**Acceptance:** these reference programs pass as `run-pass` on all three OSes:
hello world, fibonacci, FizzBuzz, `enum` + `match`, `Result` + `?`, a multi-module program,
and `extern "C"` calling `puts`.

## F2: OOP and functional core (MVP)

`class` with ARC and `weak`, `new`, `extends`/`zod`/`override`, `abstract`, interfaces with
default methods and `impl`, interfaces as types (vtables), monomorphized generics with bounds,
closures, generic `List<T>` / `Map<K, V>`, iterators with `map` / `filter`.

**Acceptance:** a reference "zoo" program using abstract classes, inheritance, `zod`,
interfaces as types, bounded generics, `List`/`Map`, lambdas and `map`/`filter` passes with
zero live ARC objects at exit and clean ASan.

## Post-MVP

- **F3 Platform:** extended standard library (fs, process, time), library outputs
  (`.a`/`.so`/`.dll`) with exports, package manager and dependencies, cross-compilation to
  Android and iOS, re-evaluate MSVC `cl.exe` and `zig cc`.
- **F4 Concurrency:** `Send`/`Sync`, `spawn`, `Mutex`, `Atomic`, atomic ARC, then
  `async`/`await`.
- **F5 Self-hosting:** rewrite the compiler in Sagitario until it compiles itself.

## Deferred decisions

Revisit after the MVP: `protected` visibility, `weak` captures in closures, operator
overloading and derived `Eq`, binding generation, internationalized diagnostics, automatic
error conversion with `From<T>` (and implementing one generic interface several times),
`Error::source()` and backtraces, a `Display` interface, slices, open ranges, general tuples,
raw strings, `@test` / `@deprecated`, static fields and mutable globals, `sag doc`.
Undecided points needed during the MVP are tracked as open questions in the
[spec index](spec/README.md#open-questions).
