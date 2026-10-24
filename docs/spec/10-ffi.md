# 10. Foreign function interface (F1)

Rationale: [ADR-0009](../adr/0009-ffi-and-unsafe.md), [ADR-0026](../adr/0026-ffi-details.md).

The C ABI is how Sagitario talks to operating systems and how mobile hosts (Swift, Kotlin
via JNI) will call Sagitario code.

```sag
use std::ffi::{c_char, c_int, CString};

extern "C" {
  fn puts(text: *const c_char) -> c_int;
}

fn main() {
  const text = CString::new("hello from C").unwrap();
  unsafe { puts(text.as_ptr()); }
}
```

## Importing C functions

- Functions declared in `extern "C" { ... }` can only be called inside an `unsafe` block.
  Their names are their C symbols; each one may be `pub`, like any function.
- A C function may be declared in several blocks, always with the same signature.
- Parameters and results must be FFI-safe (below); a function returning `()` returns `void`.
- The C standard library is always linked. A package links other C libraries and C sources
  with `[link]` in its `Nova.toml` (F3, [Nova](../nova.md#linking-c)). Variadic functions
  (`printf`) are not supported in the MVP.

## Exporting functions

```sag
@export("demo_add")
extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
```

- `extern "C" fn` gives a function the C ABI and exports it under its own name, or under the
  symbol given by `@export("symbol")`. Its parameters and result must be FFI-safe. Sagitario
  code calls it like any other function.
- A library package builds into a static and a dynamic library that export these functions,
  with a C header declaring them (F3, [Nova](../nova.md#library-outputs)).

## C symbols

A symbol (the name of an imported function, or an exported one) must be an ASCII C
identifier. It cannot be a C keyword, `main`, a name reserved by C (starting with `__` or with
`_` and an uppercase letter), a name starting with `sag_` (the runtime's) or ending in `_t`,
and each symbol is defined once.

## FFI-safe types

Integers, floats, `bool`, raw pointers and `@repr(C)` structs. `String`, `char`, enums, arrays
and other structs cannot cross the boundary: strings travel as `CString` pointers, and
characters as `u32`.

## Raw pointers

| Operation | Meaning | Needs `unsafe` |
|---|---|---|
| `*const T`, `*mut T` | Pointer types | |
| `*p` | The value `p` points to (a null pointer panics) | yes |
| `*p = value` | Writes through a `*mut T` | yes |
| `p.add(n)` | The pointer `n` elements further | yes |
| `p.is_null()` | Whether it is the null pointer | |
| `p == q`, `p != q` | Address comparison | |
| `p as *mut U`, `p as usize`, `n as *const T` | Conversions; `0 as *const T` is the null pointer | |
| `text.as_ptr()` | `*const u8` to the bytes of a `String` (not NUL-terminated) | |
| `array.as_ptr()`, `array.as_mut_ptr()` | Pointer to the first element; `as_mut_ptr` needs a `var` place | |

- A `*mut T` can be used where a `*const T` is expected.
- A pointer cannot point to a type holding `String`s or other ARC references.
- There is no address-of operator in the MVP: values C writes into are passed as arrays
  (`var out = [0i64; 1]; f(out.as_mut_ptr());`).

## `unsafe`

- `unsafe { ... }` is a block expression; inside it the program may call C functions,
  dereference raw pointers, offset them with `add` and read C strings.
- An `unsafe` block without any of these operations is a warning.
- There are no `unsafe fn` declarations in the MVP: unsafe code is wrapped in safe functions.
- Manual retain/release of class references is only possible inside `unsafe` (F2).

## `std::ffi`

The first module of the [standard library](11-standard-library.md), provided by the compiler:

| Name | Meaning |
|---|---|
| `c_char`, `c_schar`, `c_uchar` | C `char` (`i8`, or `u8` where C's `char` is unsigned), `signed char`, `unsigned char` |
| `c_short`, `c_ushort`, `c_int`, `c_uint` | `i16`, `u16`, `i32`, `u32` |
| `c_long`, `c_ulong` | 32 bits on Windows, 64 bits elsewhere |
| `c_longlong`, `c_ulonglong`, `c_float`, `c_double` | `i64`, `u64`, `f32`, `f64` |
| `c_void` | C's `void`, only behind a pointer |
| `CString` | A NUL-terminated copy of a string (ARC) |
| `string_from_c(p: *const c_char) -> String` | Copies a C string; invalid UTF-8 becomes U+FFFD. Needs `unsafe`; a null pointer panics |

- The C types are aliases of Sagitario's own types for the target, as in Rust.
- `CString::new(text) -> Result<CString, String>` fails with
  `nul byte found in provided data at position: N` when the text contains a NUL byte.
  `c.as_ptr()` is a `*const c_char` valid while `c` is alive.

## C-compatible structs

`@repr(C)` gives a struct the C layout (fields in declaration order with C's alignment). Its
fields must be FFI-safe:

```sag
@repr(C)
struct Timespec {
  pub tv_sec: i64;
  pub tv_nsec: i64;
}
```

- Class references do not cross the FFI boundary directly. They are passed as opaque handles,
  with explicit retain/release inside `unsafe` (F2).

## Post-MVP

Automatic binding generation (in the style of `bindgen`), variadic functions, function
pointers and callbacks, `unsafe fn`, `@repr(C)` enums and linking arbitrary libraries.
