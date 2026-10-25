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
2. `const`/`var`, operators with overflow checks, `as`, `if`/`else`/`while`/`loop`/`for` over
   ranges, functions with return values, template-string interpolation, ARC `String`s,
   `--release` (ADR-0020).
3. Diagnostic quality and the first broad `compile-fail` suites.
4. Structs with methods and static functions; fixed arrays.
5. Enums, exhaustive `match`, `Option`/`Result` and `?`.
6. The `String` API (methods, `StringBuilder`, `chars()`), `panic` and `run-panic` suites.
7. Multi-file modules, `use` and visibility.
8. FFI: `extern "C"`, `unsafe`, `@export`, `@repr(C)`.
9. The reference programs below.

**Acceptance:** these reference programs pass as `run-pass` on all three OSes:
hello world, fibonacci, FizzBuzz, `enum` + `match`, `Result` + `?`, a multi-module program,
and `extern "C"` calling `puts`.
They are `tests/run-pass/reference_*`: `reference_hello.sag`, `reference_fibonacci.sag`,
`reference_fizzbuzz.sag`, `reference_shapes.sag`, `reference_result.sag`,
`reference_modules/` and `reference_puts.sag`.

## F2: OOP and functional core (MVP)

`class` with ARC and `weak`, `new`, `extends`/`zod`/`override`, `abstract`, interfaces with
default methods and `impl`, interfaces as types (vtables), monomorphized generics with bounds,
closures, generic `List<T>` / `Map<K, V>`, iterators with `map` / `filter`.

Increments ([ADR-0027](adr/0027-classes-and-arc.md)):

1. Classes and ARC: `const`/`var` fields, the initializer `new`, methods and static
   functions, iterative destruction, identity `==`. With it, `self` loses `var` and the
   methods that modify a struct's `self` are inferred ([ADR-0028](adr/0028-inferred-self-mutation.md)).
2. Inheritance: `extends`, `zod`, `override`, `abstract`, dynamic dispatch and
   devirtualization ([ADR-0029](adr/0029-inheritance.md)).
3. `weak` references ([ADR-0030](adr/0030-weak-references.md)).
4. Interfaces: fields, default methods, `impl` on classes, structs and enums, interfaces as
   types, the `Error` interface ([ADR-0031](adr/0031-interfaces.md)).
5. Monomorphized generics with bounds ([ADR-0032](adr/0032-generics.md)).
6. Closures: arrow functions, function types, captures
   ([ADR-0033](adr/0033-closures-details.md)).
7. `List<T>`, `Map<K, V>`, iterators, and `Eq`/`Ord`/`Hash`
   ([ADR-0034](adr/0034-collections-and-iterators.md)).
8. The reference "zoo" program below, run with ASan and UBSan
   ([ADR-0036](adr/0036-sanitizers-and-the-reference-zoo.md)).

**Status:** done. The zoo is `tests/run-pass/reference_zoo/`.

**Acceptance:** a reference "zoo" program using abstract classes, inheritance, `zod`,
interfaces as types, bounded generics, `List`/`Map`, lambdas and `map`/`filter` passes with
zero live ARC objects at exit and clean ASan.

## F3: Platform (post-MVP)

Extended standard library (fs, process, time), library outputs (`.a`/`.so`/`.dll`) with
exports, package manager and dependencies, cross-compilation to Android and iOS, re-evaluate
MSVC `cl.exe` and `zig cc`.

Increments:

1. The standard library's foundation, `std::env`, `std::process::exit` and `std::io`
   ([ADR-0037](adr/0037-standard-library-env-io-process.md)).
2. `std::fs`: reading, writing and appending files, existence checks, removal, directories
   and the current directory; `std::path` ([ADR-0038](adr/0038-file-system-and-paths.md)).
3. `std::time` (`Instant`, `Duration`, system time) and `std::process::Command` (run a program,
   capture its output and exit status) ([ADR-0039](adr/0039-time-and-commands.md)).
4. Packages with Nova, the package manager: `Nova.toml`, `nova new`, `nova build`/`run`/`check`
   with `src/main.sag` or `src/lib.sag` ([ADR-0040](adr/0040-nova-package-manager.md)).
5. Dependencies: packages by path, dependency names as path roots, no cycles between packages
   ([ADR-0041](adr/0041-dependencies-by-path.md)). `Nova.lock` waits for dependencies from git or
   a registry.
6. Library outputs: a library builds `.a`/`.so`/`.dylib`/`.dll` with the exported functions and
   a generated C header; `[link]` for C libraries and sources
   ([ADR-0042](adr/0042-library-outputs-and-linking-c.md)).
7. Cross-compilation: `--target`, Android (NDK) and iOS, with `c_long`/`c_char` per target
   ([ADR-0043](adr/0043-cross-compilation.md)).
8. MSVC `cl` supported best-effort, `zig cc` not adopted, `c_char` spelled `char` in headers,
   and the acceptance program ([ADR-0044](adr/0044-c-toolchains.md)).

**Status:** done. The acceptance program is `tests/acceptance/f3/`, run by the Nova test
`f3_acceptance`.

**Acceptance:** a command-line tool (a small `grep` or `wc`) built as a package reads files and
stdin, depends on a local library package and passes with sanitizers; the same library builds
as `.a`/`.so`/`.dll` and is called from a C program; and the tool compiles (without running)
for Android and iOS. iOS runs no standalone programs (ADR-0043), so there the library builds
instead.

## F4: Concurrency (post-MVP)

Threads with data races prevented by `Send`, synchronization and message passing
([ADR-0045](adr/0045-threads-and-send.md)).

Increments:

1. `Send` inferred by the compiler, `std::thread` (`spawn`, `JoinHandle::join`), atomic
   reference counts in programs using threads, and `--sanitize=thread`
   ([ADR-0045](adr/0045-threads-and-send.md)).
2. `std::sync`: `Mutex<T>` and atomics per type (`AtomicI64`, ...)
   ([ADR-0046](adr/0046-mutex-and-atomics.md)).
3. `Channel<T>`, `thread::sleep` and `thread::available_parallelism`
   ([ADR-0047](adr/0047-channels-and-thread-utilities.md)).
4. The design of `async`/`await`, for F6, and the acceptance program
   ([ADR-0048](adr/0048-async-await.md)).

**Status:** done. The acceptance program is `tests/run-pass/reference_wordcount/`.

**Acceptance:** a parallel word count: threads count files and return `Map`s through `join`, an
`Atomic` tracks progress, and a producer feeds consumers through a `Channel`; it passes with
ThreadSanitizer on Linux and macOS, and with the leak check.

## F5: Self-hosting (post-MVP)

Rewrite the compiler in Sagitario until it compiles itself
([ADR-0049](adr/0049-self-hosting-plan-and-language-prerequisites.md)). Each pass is checked
against the Rust compiler's on every program under `tests/`.

Increments:

1. Language prerequisites: module-level constants, `include_str`, `if let` and `while let`.
2. Dumps for differential tests (`--dump=tokens`), and `span`, `diagnostics` and `lexer`;
   `char::from_u32` ([ADR-0050](adr/0050-lexer-port-and-token-dumps.md)).
3. `parser`, with AST dumps (`--dump=ast`) ([ADR-0051](adr/0051-parser-port-and-ast-dumps.md)).
4. `resolve` and the module loader, with resolution dumps (`--dump=resolve`)
   ([ADR-0052](adr/0052-resolver-port-and-resolution-dumps.md)).
5. `types`, in two halves with their dumps: 5a the declarations (`--dump=declarations`), 5b the
   bodies of functions (`--dump=types`) ([ADR-0053](adr/0053-type-checker-port-and-type-dumps.md)).
6. `sir` and `codegen_c`, compared through the generated C (`--dump=c`)
   ([ADR-0054](adr/0054-lowering-and-c-generation-port.md)).
7. The driver and the CLI: `sagc` passes every suite (`SAG_COMPILER`), and executables reserve
   an 8 MiB stack on Windows ([ADR-0055](adr/0055-driver-and-cli-port.md)).
8. The bootstrap to the fixed point, its CI job and the acceptance.

**Acceptance:** `sagc` built by the Rust compiler (stage 1) passes every suite on Windows, Linux
and macOS with clang, gcc and MinGW, and `sagc` built by stage 1 (stage 2) generates the same C
for the compiler as stage 1.

## Later phases (post-MVP)

- **F6 `async`/`await`:** stackless tasks (`Task<T>`) on a single-threaded executor per thread,
  as designed in [ADR-0048](adr/0048-async-await.md).

## Deferred decisions

Revisit after the MVP: shared mutable collections between threads (ADR-0045), bounded
channels, `try_receive` and timeouts (ADR-0047), `protected` visibility, `weak` captures in closures, operator
overloading and derived `Eq`, binding generation, internationalized diagnostics, automatic
error conversion with `From<T>` (and implementing one generic interface several times),
`Error::source()` and backtraces, a `Display` interface, slices, open ranges, general tuples,
raw strings, `@test` / `@deprecated`, static fields and mutable globals, `sag doc`, inference of `None`/`Ok`/`Err` from later uses, re-exports (`pub use`), glob
imports, importing enum variants or a type's functions, `use` inside blocks.
Undecided points needed during the MVP are tracked as open questions in the
[spec index](spec/README.md#open-questions).
