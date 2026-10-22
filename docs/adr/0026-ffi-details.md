# 0026. FFI details: pointers, `unsafe`, `std::ffi` and exports

- Status: Accepted
- Date: 2026-10-22
- Refines: [ADR-0009](0009-ffi-and-unsafe.md)

## Context

ADR-0009 and the specification fixed the FFI's shape (`extern "C"` blocks callable in
`unsafe`, `extern "C" fn` with `@export`, raw pointers dereferenced only in `unsafe`, C types in
an `ffi` module, `@repr(C)`) but not the operations on pointers, how strings cross the
boundary, which types are allowed, or how symbols are checked (question Q24, F1 increment 8).

## Decision

| # | Topic | Decision |
|---|---|---|
| 1 | Dereference | Rust syntax: `*p` reads and `*p = v` writes (only through `*mut`), both inside `unsafe`. A null pointer is checked and panics. |
| 2 | Pointer operations | `p.is_null()` (safe), `p.add(n)` (in `unsafe`), `as` between pointer types and between pointers and `usize`; the null pointer is `0 as *const T`. `==`/`!=` compare addresses; a `*mut T` can be used where a `*const T` is expected. |
| 3 | Getting pointers | `text.as_ptr()` on `String` (`*const u8`), `arr.as_ptr()` and `arr.as_mut_ptr()` on arrays (the latter on `var` places). No address-of operator in the MVP. |
| 4 | FFI-safe types | Integers, floats, `bool`, raw pointers, `@repr(C)` structs, and `()` as a result. Pointers cannot point to types holding ARC references. |
| 5 | `std::ffi` | The first piece of the standard library, provided by the compiler: `c_char`, `c_schar`, `c_uchar`, `c_short`, `c_ushort`, `c_int`, `c_uint`, `c_long`, `c_ulong`, `c_longlong`, `c_ulonglong`, `c_float`, `c_double` (aliases of the target's types), `c_void` (only behind pointers), `CString` and `string_from_c`. |
| 6 | Strings | `CString::new(text) -> Result<CString, String>` (Rust's error for an interior NUL), an ARC reference whose `as_ptr()` is a `*const c_char`; `string_from_c(p)` copies a C string, replacing invalid UTF-8 with U+FFFD, inside `unsafe`. |
| 7 | `unsafe` | A block expression; one without unsafe operations is a warning. No `unsafe fn` in the MVP. |
| 8 | Exports | `extern "C" fn f` has the C ABI and is exported as `f`, or as the symbol given by `@export("symbol")`; Sagitario code calls it as any function. |
| 9 | Symbols | A C symbol must be an ASCII C identifier, not a C keyword or reserved name (`__x`, `_X`, `main`, names starting with `sag_` or ending in `_t`), and used once; a C function may be declared again only with the same signature. |
| 10 | Attributes | Unknown, repeated or misplaced attributes are errors; `@test`, `@deprecated` and `@repr(C)` on enums are not implemented yet. |
| 11 | Out of the MVP | Variadic functions, linking libraries other than the C library, function pointers and callbacks, `unsafe fn`, enum layouts. |

## Alternatives considered

- **`&x` / `&raw mut x` to take addresses**: needs rules for borrowing locals; arrays and
  `CString` cover the MVP's needs.
- **C types as distinct types**: would force casts between `c_int` and `i32` everywhere; Rust
  also defines them as aliases.
- **Declaring C functions in the program's own C file**: the runtime header includes system
  headers, whose declarations can conflict with the program's (`char *` against
  `*const c_char`). Calls go through a separate C file with no system header.

## Consequences

- `sag` generates a second C file (`sag_ffi.c`, or `<name>_ffi.c` with `--emit-c`) holding a
  wrapper per C function and the exported symbols.
- `c_long` and `c_char` follow the machine running the compiler until cross-compilation (F3).
