# 0001. Compile to C11 through an own IR (SIR)

- Status: Accepted
- Date: 2026-10-21

## Context

Sagitario must reach every operating system, including iOS and Android, and must eventually
be self-hosting. The backend therefore has to be portable, high-performance, and
re-implementable in Sagitario itself. The primary development machine runs Windows.

## Decision

The pipeline is `source → AST → typed AST → SIR → C11`. SIR is Sagitario's own intermediate
representation, with ARC retain/release made explicit. The MVP has a single backend that
emits portable C11, compiled by the platform's C compiler. An LLVM backend may be added
later behind SIR without touching the frontend.

## Alternatives considered

- **LLVM directly**: best optimizer and target coverage, but painful to build/link on
  Windows, pins an LLVM version, and self-hosting would need FFI to the LLVM C API.
- **Cranelift**: pure Rust, but weaker optimization, limited mobile support, and unusable
  from Sagitario (it is a Rust library), which blocks self-hosting.
- **Own machine-code backend**: years of work and poor code quality initially.

## Consequences

- Every target the C toolchains support (MSVC/clang/gcc, Android NDK, Xcode) is reachable.
- The self-hosted compiler only needs to emit text.
- Near-C performance, delegated to the C optimizer.
- Weaker debug info initially (mitigated with `#line`), slower compiles than Cranelift, and
  users need a C compiler installed.
- SIR is the natural place for ARC optimizations and devirtualization.
