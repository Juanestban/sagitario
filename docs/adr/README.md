# Architecture Decision Records

Each ADR records **why** a decision was made and which alternatives were rejected. The rules
themselves live in the [specification](../spec/README.md) and in
[architecture.md](../architecture.md).

ADRs are immutable once accepted. To change a decision, write a new ADR that supersedes the
old one, set the old one's status to `Superseded by ADR-NNNN`, and update the spec.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-compile-to-c-via-sir.md) | Compile to C11 through an own IR (SIR) | Accepted |
| [0002](0002-arc-memory-model.md) | ARC memory model with value and reference types | Accepted (refined by 0027) |
| [0003](0003-static-typing-and-generics.md) | Static typing, local inference, monomorphized generics | Accepted (refined by 0032) |
| [0004](0004-class-model.md) | Class model: single inheritance, interfaces, no open/final | Accepted (refined by 0027, 0029, 0031) |
| [0005](0005-result-error-handling.md) | Errors as values with `Result` and `?` | Accepted (refined by 0031, 0033) |
| [0006](0006-no-null-option.md) | No null; `Option<T>` only | Accepted |
| [0007](0007-mutability-const-var.md) | Mutability with `const` / `var` | Accepted (amended by 0028; refined by 0049) |
| [0008](0008-concurrency-deferred.md) | Single-threaded MVP; Rust-style concurrency later | Accepted (superseded in part by 0045) |
| [0009](0009-ffi-and-unsafe.md) | Rust-style FFI and `unsafe` | Accepted (refined by 0026) |
| [0010](0010-modules-and-packages.md) | File-as-module, logical imports, packages | Accepted (amended by 0024, 0025; manifest superseded by 0040) |
| [0011](0011-surface-syntax.md) | Surface syntax | Accepted (refined by 0035) |
| [0012](0012-closures.md) | Closures capture by value, read-only | Accepted (refined by 0033) |
| [0013](0013-primitive-semantics.md) | Semantics of primitives and `String` | Accepted (refined by 0034, 0050) |
| [0014](0014-mvp-scope.md) | MVP scope: F0 + F1 + F2, desktop only | Accepted (refined by 0049) |
| [0015](0015-compiler-architecture-and-toolchain.md) | Compiler architecture and C toolchain | Accepted (refined by 0044) |
| [0016](0016-engineering-process.md) | Testing, quality gates, conventions and license | Accepted (refined by 0036) |
| [0017](0017-resolution-of-open-questions-q1-q11.md) | Resolution of open questions Q1–Q11 | Accepted (refined by 0034) |
| [0018](0018-windows-c-toolchain.md) | Windows C toolchain: official LLVM (MSVC target), MinGW clang tolerated | Accepted (refined by 0036, 0044) |
| [0019](0019-prelude-and-printing.md) | Prelude and printing | Accepted (refined by 0037) |
| [0020](0020-expressions-bindings-and-build-modes.md) | Operators, bindings, control flow, interpolation and build modes | Accepted |
| [0021](0021-structs-and-fixed-arrays.md) | Structs, methods and fixed arrays | Accepted (amended by 0028) |
| [0022](0022-enums-match-option-result.md) | Enums, `match`, `Option`/`Result` and `?` | Accepted |
| [0023](0023-string-api-and-panic.md) | The `String` API, `StringBuilder` and `panic` | Accepted |
| [0024](0024-modules-imports-and-visibility.md) | Modules, imports, visibility and the entry point | Accepted (amended by 0025; refined by 0037) |
| [0025](0025-single-pub-visibility.md) | A single `pub` visibility, without `pub(pkg)` | Accepted |
| [0026](0026-ffi-details.md) | FFI details: pointers, `unsafe`, `std::ffi` and exports | Accepted (refined by 0043) |
| [0027](0027-classes-and-arc.md) | Classes, construction, ARC objects and `weak` references | Accepted (refined by 0030, 0035) |
| [0028](0028-inferred-self-mutation.md) | `self` without `var`: methods that modify `self` are inferred | Accepted |
| [0029](0029-inheritance.md) | Inheritance: construction, overriding, subtyping and dispatch | Accepted |
| [0030](0030-weak-references.md) | Weak references: initialization, scope and runtime | Accepted |
| [0031](0031-interfaces.md) | Interfaces: fields, implementation, interface values and `Error` | Accepted (completed by 0032) |
| [0032](0032-generics.md) | Generics: type arguments, inference, bounds and monomorphization | Accepted (refined by 0034) |
| [0033](0033-closures-details.md) | Closures: arrow functions, function values and their representation | Accepted |
| [0034](0034-collections-and-iterators.md) | Collections, iterators, and equality, order and hashing | Accepted |
| [0035](0035-explicit-self-in-initializers.md) | Initializers declare `self` | Accepted |
| [0036](0036-sanitizers-and-the-reference-zoo.md) | Sanitizers and the reference "zoo" program | Accepted |
| [0037](0037-standard-library-env-io-process.md) | The standard library: `std::env`, `std::io` and `std::process` | Accepted |
| [0038](0038-file-system-and-paths.md) | `std::fs` and `std::path` | Accepted |
| [0039](0039-time-and-commands.md) | `std::time` and `std::process::Command` | Accepted (refined by 0047) |
| [0040](0040-nova-package-manager.md) | Nova, the package manager | Accepted |
| [0041](0041-dependencies-by-path.md) | Dependencies by path | Accepted |
| [0042](0042-library-outputs-and-linking-c.md) | Library outputs and linking C | Accepted (refined by 0044) |
| [0043](0043-cross-compilation.md) | Cross-compilation to Android and iOS | Accepted |
| [0044](0044-c-toolchains.md) | C toolchains: MSVC `cl` best-effort, no `zig cc`, and the F3 acceptance program | Accepted |
| [0045](0045-threads-and-send.md) | Threads and `Send` | Accepted (refined by 0046, 0047, 0048) |
| [0046](0046-mutex-and-atomics.md) | `std::sync`: `Mutex` and atomics | Accepted |
| [0047](0047-channels-and-thread-utilities.md) | Channels and thread utilities | Accepted |
| [0048](0048-async-await.md) | `async`/`await`, for phase F6 | Accepted (not implemented; moved to F7 by 0057, to F8 by 0063) |
| [0049](0049-self-hosting-plan-and-language-prerequisites.md) | The self-hosting plan, module-level constants, `include_str`, `if let` and `while let` | Accepted (refined by 0050) |
| [0050](0050-lexer-port-and-token-dumps.md) | The lexer port, token dumps and `char::from_u32` | Accepted (refined by 0051) |
| [0051](0051-parser-port-and-ast-dumps.md) | The parser port and AST dumps | Accepted (refined by 0052) |
| [0052](0052-resolver-port-and-resolution-dumps.md) | The resolver port and resolution dumps | Accepted (refined by 0053) |
| [0053](0053-type-checker-port-and-type-dumps.md) | The type checker port and type dumps | Accepted (refined by 0054) |
| [0054](0054-lowering-and-c-generation-port.md) | The lowering and C generation port | Accepted (refined by 0055) |
| [0055](0055-driver-and-cli-port.md) | The driver and CLI port | Accepted (refined by 0056; Q277 superseded by 0061) |
| [0056](0056-bootstrap-and-acceptance.md) | The bootstrap and the acceptance of F5 | Accepted |
| [0057](0057-editor-tooling-and-independence-from-rust.md) | Editor tooling and independence from Rust (phase F6) | Accepted (refined by 0058) |
| [0058](0058-language-server.md) | The language server | Accepted (refined by 0059, 0062) |
| [0059](0059-language-server-paths-and-semantic-tokens.md) | Paths and semantic tokens in the language server | Accepted |
| [0060](0060-driver-parity-for-nova.md) | The pivot without Rust, and the driver for Nova | Accepted (refined by 0064) |
| [0061](0061-terminals.md) | Terminals: `io::is_terminal` and colored diagnostics in `sagc` | Accepted |
| [0062](0062-language-server-std-sources-and-lone-files.md) | The language server on the standard library and on lone files | Accepted |
| [0063](0063-distribution-phase-and-roadmap-v2.md) | Distribution as phase F7, and the roadmap v2 of a game engine | Accepted |
| [0064](0064-nova-in-sagitario.md) | Nova in Sagitario | Accepted |

## Template

```markdown
# NNNN. Title

- Status: Proposed | Accepted | Superseded by ADR-NNNN
- Date: YYYY-MM-DD

## Context
## Decision
## Alternatives considered
## Consequences
```
