# Architecture Decision Records

Each ADR records **why** a decision was made and which alternatives were rejected. The rules
themselves live in the [specification](../spec/README.md) and in
[architecture.md](../architecture.md).

ADRs are immutable once accepted. To change a decision, write a new ADR that supersedes the
old one, set the old one's status to `Superseded by ADR-NNNN`, and update the spec.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-compile-to-c-via-sir.md) | Compile to C11 through an own IR (SIR) | Accepted |
| [0002](0002-arc-memory-model.md) | ARC memory model with value and reference types | Accepted |
| [0003](0003-static-typing-and-generics.md) | Static typing, local inference, monomorphized generics | Accepted |
| [0004](0004-class-model.md) | Class model: single inheritance, interfaces, no open/final | Accepted |
| [0005](0005-result-error-handling.md) | Errors as values with `Result` and `?` | Accepted |
| [0006](0006-no-null-option.md) | No null; `Option<T>` only | Accepted |
| [0007](0007-mutability-const-var.md) | Mutability with `const` / `var` | Accepted |
| [0008](0008-concurrency-deferred.md) | Single-threaded MVP; Rust-style concurrency later | Accepted |
| [0009](0009-ffi-and-unsafe.md) | Rust-style FFI and `unsafe` | Accepted |
| [0010](0010-modules-and-packages.md) | File-as-module, logical imports, packages | Accepted |
| [0011](0011-surface-syntax.md) | Surface syntax | Accepted |
| [0012](0012-closures.md) | Closures capture by value, read-only | Accepted |
| [0013](0013-primitive-semantics.md) | Semantics of primitives and `String` | Accepted |
| [0014](0014-mvp-scope.md) | MVP scope: F0 + F1 + F2, desktop only | Accepted |
| [0015](0015-compiler-architecture-and-toolchain.md) | Compiler architecture and C toolchain | Accepted |
| [0016](0016-engineering-process.md) | Testing, quality gates, conventions and license | Accepted |
| [0017](0017-resolution-of-open-questions-q1-q11.md) | Resolution of open questions Q1–Q11 | Accepted |
| [0018](0018-windows-c-toolchain.md) | Windows C toolchain: official LLVM (MSVC target), MinGW clang tolerated | Accepted |
| [0019](0019-prelude-and-printing.md) | Prelude and printing | Accepted |
| [0020](0020-expressions-bindings-and-build-modes.md) | Operators, bindings, control flow, interpolation and build modes | Accepted |
| [0021](0021-structs-and-fixed-arrays.md) | Structs, methods and fixed arrays | Accepted |
| [0022](0022-enums-match-option-result.md) | Enums, `match`, `Option`/`Result` and `?` | Accepted |
| [0023](0023-string-api-and-panic.md) | The `String` API, `StringBuilder` and `panic` | Accepted |
| [0024](0024-modules-imports-and-visibility.md) | Modules, imports, visibility and the entry point | Accepted |

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
