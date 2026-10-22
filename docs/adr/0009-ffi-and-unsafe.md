# 0009. Rust-style FFI and `unsafe`

- Status: Accepted; refined by [ADR-0026](0026-ffi-details.md)
- Date: 2026-10-21

## Context

OS APIs are reached through C, and iOS/Android hosts can only call native code through the C
ABI. The standard library itself is built on FFI.

## Decision

- Import with `extern "C" { ... }`; calls require `unsafe`.
- Export with `extern "C" fn` plus an export attribute, enabling static/dynamic libraries.
- Raw pointers `*const T` / `*mut T`, dereferenced only in `unsafe`.
- C types in an `ffi` module; C layout for structs via a `repr(C)` attribute.
- ARC class references cross the boundary only as opaque handles with explicit
  retain/release.
- Automatic binding generation is post-MVP.

## Alternatives considered

- **No `unsafe` marker, FFI freely callable**: hides where memory safety can be broken.

## Consequences

- Clear audit boundary for unsafe code.
- Mobile embedding becomes possible once library build modes exist (F3).
