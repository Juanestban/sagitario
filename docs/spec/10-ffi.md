# 10. Foreign function interface (F1)

Rationale: [ADR-0009](../adr/0009-ffi-and-unsafe.md), [ADR-0026](../adr/0026-ffi-details.md),
[ADR-0078](../adr/0078-ffi-without-c-shims.md).

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

## Function pointers

Rationale: [ADR-0078](../adr/0078-ffi-without-c-shims.md) (increment 1).

```sag
extern "C" {
  fn qsort(base: *mut c_void, count: usize, size: usize, compare: extern "C" fn(*const c_void, *const c_void) -> c_int);
}

extern "C" fn by_value(a: *const c_void, b: *const c_void) -> c_int { ... }

unsafe { qsort(numbers.as_mut_ptr() as *mut c_void, numbers.len(), 4, by_value); }
```

- `extern "C" fn(A, B) -> R` is the type of a C function pointer, and
  `unsafe extern "C" fn(A, B) -> R` that of one that needs `unsafe` to be called. `-> R` is left
  out for `void`. They are FFI-safe, are copied like integers, and are distinct from `fn(A) -> R`,
  the type of closures, which never cross the boundary.
- An `extern "C" fn` of the program is a value of type `extern "C" fn(..)` (or
  `unsafe extern "C" fn(..)` when declared `unsafe extern "C" fn`); where a closure type is
  expected, a safe one is a closure as before. A function of an `extern` block is a value of type
  `unsafe extern "C" fn(..)`. Lambdas, methods and generic functions are not C function pointers.
- `f(args)` calls a function pointer, also one held in a field (`callbacks.on_value(x)`). Calling
  an `unsafe extern "C" fn` needs `unsafe`. An `extern "C" fn` can be used where an
  `unsafe extern "C" fn` with the same signature is expected.
- A function pointer is never null. `Option<extern "C" fn(..)>` has the size of a pointer, with
  `None` as `NULL`, and is FFI-safe: nullable callbacks and results are written with it.
- `as` converts a function pointer to `usize` or to a raw pointer. Inside `unsafe`, it converts
  between function pointer types, and from a raw pointer or a `usize` to a function pointer; a
  null address panics (`null C function pointer`). Function pointers are not compared with `==`
  nor interpolated; compare them as `usize`.
- C may call an `extern "C" fn` of the program through a pointer, also on a thread of its own: a
  program that uses one as a value counts references atomically, as programs using
  `std::thread` do. A panic inside it ends the process with exit code 101; it never unwinds into C.

## `unsafe fn`

```sag
/// Halves `x`; the caller must pass an even number.
unsafe fn halve(x: i32) -> i32 { x / 2 }

const half = unsafe { halve(10) };
```

- `unsafe fn`, `pub unsafe fn`, `unsafe extern "C" fn` and `unsafe` methods may only be
  called inside `unsafe`. Their bodies are not unsafe by themselves: unsafe operations inside them
  need their own `unsafe` blocks, as in Rust 2024. A `///` comment should say what the caller must
  guarantee.
- An `unsafe fn` cannot be used as a closure; wrap the call in a lambda with an `unsafe` block.
- An override is `unsafe` exactly when the method it overrides is, and an `unsafe fn` cannot
  implement a method of an interface.
- `unsafe async fn` and `unsafe` blocks of C functions (`unsafe extern "C" { }`) are not
  supported.

## C symbols

A symbol (the name of an imported function, or an exported one) must be an ASCII C
identifier. It cannot be a C keyword, `main`, a name reserved by C (starting with `__` or with
`_` and an uppercase letter), a name starting with `sag_` (the runtime's) or ending in `_t`,
and each symbol is defined once.

## FFI-safe types

Integers, floats, `bool`, raw pointers, references to FFI-safe types (non-null pointers for C),
C function pointers (also in an `Option`), `@repr(C)` structs and unions, and enums with an
integer representation. Fields of `@repr(C)` structs and
unions may also be fixed arrays of these (`[c_char; 256]`), laid out as C's `T name[N]`; an array
is never a parameter or a result, since C passes a pointer to its first element instead. `String`, `char`, enums, arrays
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
| `list.as_ptr()`, `list.as_mut_ptr()` | Pointer to the first element of a list, valid until its length changes | |
| `&raw const place`, `&raw mut place` | The address of a place, as `*const T` or `*mut T` | |
| `reference as *const T` | A reference parameter as a raw pointer (`&mut T` also as `*mut T`) | |

- A `*mut T` can be used where a `*const T` is expected.
- A pointer cannot point to a type holding `String`s or other ARC references.
- `&raw const` and `&raw mut` ([ADR-0078](../adr/0078-ffi-without-c-shims.md), increment 3), as
  in Rust, take the address of a place: a binding, a field (also of a class instance or a union),
  an element of an array, or `*pointer`. `&raw mut` needs a place that can be modified, and counts
  as modifying its binding. An element of a list is not a place for them, since the elements move
  when the list grows: use `list.as_mut_ptr()`. Taking an address is safe; using it needs `unsafe`.
- C fills buffers of lists: `count` with `&raw mut count`, then `list.resize(count, value)` and
  `list.as_mut_ptr()`. `fs::read` gives the bytes of a file, such as SPIR-V code, as a `List<u8>`, and
  `fs::write_bytes` writes a slice of them.
- [References](03-memory.md#references) meet C as pointers, as in Rust
  ([ADR-0079](../adr/0079-references-and-borrowing.md), Q496): a `&T` can be passed where a
  `*const T` is expected and a `&mut T` where a `*mut T` is (`frexp(x, &mut exponent)`), and
  `v as *const T` converts a reference parameter. In an `extern "C" fn`, an `extern` block or a C
  function pointer type, a reference parameter is a pointer C must never pass as null:
  `extern "C" fn compare(a: &i32, b: &i32) -> c_int` can be given to `qsort`.

## `unsafe`

- `unsafe { ... }` is a block expression; inside it the program may call C functions,
  dereference raw pointers, offset them with `add` and read C strings.
- Calling an `unsafe fn` or an `unsafe extern "C" fn` pointer, the conversions of function
  pointers above, and using a [`static var`](03-memory.md#global-state-g1), are unsafe
  operations too. A `static var` is how a callback that C calls without a context keeps state,
  and `&raw mut NAME` passes one to C.
- An `unsafe` block without any of these operations is a warning.
- Manual retain/release of class references is only possible inside `unsafe` (F2).

## `std::ffi`

The sizes of these types, and of `isize` and `usize`, are those of the target the program is
compiled for ([targets](../nova.md#targets)), not of the machine running the compiler.

The first module of the [standard library](11-standard-library.md), provided by the compiler:

| Name | Meaning |
|---|---|
| `c_char`, `c_schar`, `c_uchar` | C `char` (`i8`, or `u8` where C's `char` is unsigned: Linux and Android on ARM), `signed char`, `unsigned char` |
| `c_short`, `c_ushort`, `c_int`, `c_uint` | `i16`, `u16`, `i32`, `u32` |
| `c_long`, `c_ulong` | 32 bits on Windows and on 32-bit targets, 64 bits elsewhere |
| `c_longlong`, `c_ulonglong`, `c_float`, `c_double` | `i64`, `u64`, `f32`, `f64` |
| `c_void` | C's `void`, only behind a pointer |
| `CString` | A NUL-terminated copy of a string (ARC) |
| `string_from_c(p: *const c_char) -> String` | Copies a C string; invalid UTF-8 becomes U+FFFD. Needs `unsafe`; a null pointer panics |

- The C types are aliases of Sagitario's own types for the target, as in Rust.
- `Library` loads dynamic libraries ([below](#dynamic-libraries)).
- `CString::new(text) -> Result<CString, String>` fails with
  `nul byte found in provided data at position: N` when the text contains a NUL byte.
  `c.as_ptr()` is a `*const c_char` valid while `c` is alive.

## Dynamic libraries

Rationale: [ADR-0078](../adr/0078-ffi-without-c-shims.md) (increment 4).

```sag
use std::ffi::{c_int, Library};

const vulkan = Library::open("vulkan-1.dll")?;
const found = vulkan.symbol("vkGetInstanceProcAddr").unwrap();
const get_proc = unsafe { found as unsafe extern "C" fn(*mut c_void, *const c_char) -> Option<unsafe extern "C" fn()> };
```

| Function | Result |
|---|---|
| `Library::open(path) -> Result<Library, String>` | Loads a library by its path, or by its name as the system finds it (`LoadLibraryW`, `dlopen`); the error names the path and the system's reason |
| `library.symbol(name) -> Option<unsafe extern "C" fn()>` | The function `name`, or `None`; converted to its own type with `as` inside `unsafe` |

- A library is never unloaded: the function pointers taken from it stay valid until the program
  ends. Linking a program with `[link]` remains the way for libraries it always needs.
- `Library` is written in Sagitario (`compiler/std/ffi.sag`) on functions of the runtime.

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

## Unions

Rationale: [ADR-0078](../adr/0078-ffi-without-c-shims.md) (increment 2).

```sag
@repr(C)
union ClearValue {
  pub color: [f32; 4];
  pub depth_stencil: DepthStencil;
}

var value = ClearValue { color: [0.0, 0.0, 0.0, 1.0] };
value.depth_stencil = DepthStencil { depth: 1.0, stencil: 0 };
const depth = unsafe { value.depth_stencil };
```

- A union's fields share their memory, as in C. It needs `@repr(C)`, and its fields are FFI-safe
  (so they hold no reference-counted values).
- A literal sets exactly one field. Writing a field with `=` is safe; reading a field (also to
  change part of it, or with `+=`) needs `unsafe`, since the union may hold another one.
- A union is a value type like a struct: copied when assigned, changed through `var` bindings.
  Generic unions, unions implementing interfaces and functions of unions are not supported.

## Enums with an integer representation

```sag
@repr(i32)
enum Result {
  Success = 0,
  NotReady,
  OutOfMemory = -1,
}

const code = Result::NotReady as i32;
const back: Option<Result> = Result::from_raw(code);
```

- `@repr(i8)` to `@repr(u64)`, `@repr(isize)` and `@repr(usize)` give an enum without data that
  integer type, and `@repr(C)` gives it `i32`, C's `int`. Its values are laid out as that integer
  and are FFI-safe.
- A variant may take its value with `= N` or `= -N`; the first one is `0` otherwise, and each
  other one the previous plus one. A value must fit in the type and be unique. Explicit values
  need a representation, and an enum with one cannot be generic nor have variants with data.
- `as` converts a value to any integer type. `Name::from_raw(n) -> Option<Name>` converts an
  integer of the representation, giving `None` for a value that is no variant.
- A value C stores that is no variant (read through a pointer or from a C structure) is not
  undefined behavior: matching or comparing it panics with
  `invalid value 9 for enum Name`. C enumerations whose values combine as flags are written as
  integer constants instead.

## Not yet

Automatic binding generation (in the style of `bindgen`, or from `vk.xml`), variadic functions,
bitfields and packed structs. C++ libraries are used through their C interfaces.
