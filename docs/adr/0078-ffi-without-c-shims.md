# 0078. A complete FFI: C libraries without C shims

- Status: Proposed
- Date: 2026-11-09
- Refines: [ADR-0026](0026-ffi-details.md) (items out of the MVP, row 11),
  [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (G1 of roadmap v2)

## Context

The FFI of the MVP (ADR-0009, ADR-0026) calls C functions with numbers, raw pointers and
`@repr(C)` structs. That is enough for libc, but not for the libraries a game engine is built on:
Vulkan is reached through function pointers loaded at run time (`vkGetInstanceProcAddr`),
reports through a callback (`pfnUserCallback`), and its structures hold fixed arrays
(`deviceName[256]`), unions (`VkClearValue`) and enumerations; Win32 needs a window procedure, and
shaders are read from binary SPIR-V files. Today each of these needs a C file in `[link] sources`.

The goal of Sagitario is to be a complete language, as Rust is: a program, or a package wrapping a
C library, is written in Sagitario alone, and C and C++ only appear as the libraries it links.
The wrapper of Vulkan itself belongs to a separate project; this record gives the language what
that project needs. Rust is the reference for every form, as it is for the FFI of the MVP. The
safe references of Rust (`&T`, `&mut T`) are a separate record
([ADR-0079](0079-references-and-borrowing.md)).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q469 | Increments | **1** function pointers and `unsafe fn`; **2** layouts: fixed arrays in `@repr(C)` structs, unions, enums with an integer representation, `std::mem`; **3** addresses and buffers: `&raw const` / `&raw mut`, pointers to the elements of lists, `List::resize`, `fs::read`; **4** loading dynamic libraries; **5** the acceptance. Each increment updates the spec and has its own tests. |
| Q470 | Function pointer types | `extern "C" fn(A, B) -> R` and `unsafe extern "C" fn(A, B) -> R`, as in Rust: a C function pointer, distinct from `fn(A) -> R` (a closure with an ARC environment, which never crosses the boundary). `-> R` is omitted for `void`. They are FFI-safe, may be parameters, results, fields of `@repr(C)` structs, locals and type arguments, and are copied like integers. |
| Q471 | Their values | An item `extern "C" fn f` of Sagitario is a value of type `extern "C" fn(..)`; a function declared in an `extern "C" { }` block is a value of type `unsafe extern "C" fn(..)` (lifting the `E0314` of ADR-0033 for them). Lambdas, methods and generic functions are not C function pointers. |
| Q472 | Calling | `f(args)` calls a function pointer. Calling an `unsafe extern "C" fn` needs `unsafe`; calling an `extern "C" fn` does not, as in Rust. |
| Q473 | Null | A function pointer is never null. `Option<extern "C" fn(..)>` (and of `unsafe extern "C" fn`) has the size of a pointer, with `None` as `NULL`, and is FFI-safe: that is how a nullable callback field or a result such as `PFN_vkVoidFunction` is written. |
| Q474 | Conversions | `as` between function pointer types, and between a function pointer and `usize`, `*const T` or `*mut T`, only inside `unsafe` (Rust's `transmute`); a function pointer to a pointer or `usize` is safe. Function pointers are not compared with `==`; compare them as `usize`. The C code converts between function and data pointers by copying their bits (`memcpy`, with a static assertion that both have the same size), never with a cast, which ISO C11 does not define. |
| Q475 | Callbacks | C may call an `extern "C" fn` of Sagitario through a pointer. A panic inside it prints its message and ends the process with exit code 101, as for exported functions: it never unwinds into C. C may call it on a thread of its own (Vulkan's drivers do), so a program that uses an `extern "C" fn` of Sagitario as a value is compiled with atomic reference counts, as programs using `std::thread` are (ADR-0045). |
| Q476 | `unsafe fn` | As in Rust 2024: `unsafe fn f()` (also `pub`, methods and `unsafe extern "C" fn` items) may only be called inside `unsafe`; its body is not an unsafe context by itself, and needs `unsafe { }` blocks for its own unsafe operations. A documentation comment should say what the caller must guarantee. |
| Q477 | Arrays in `@repr(C)` | A field of a `@repr(C)` struct (or union) may be a fixed array `[T; N]` of an FFI-safe `T`, laid out as C's `T name[N]`. Arrays still cannot be parameters or results of C functions (C passes a pointer: `array.as_ptr()`). |
| Q478 | Unions | `@repr(C) union Name { pub a: T; pub b: U; }`, as Rust's: every field FFI-safe and without ARC references. A literal sets exactly one field (`ClearValue { color: c }`). Writing a field is safe; reading one needs `unsafe`. A union has no methods of its own in this record, and is a value type like a struct. Unions without `@repr(C)` are not allowed. |
| Q479 | Enums with an integer representation | `@repr(i32)` (any integer type) on an enum without data allows explicit discriminants, `Name = 7` (the others follow the previous one plus one, the first is `0`), and `as` to integers. Such an enum is FFI-safe and laid out as its integer. A value C stores that is no variant is not undefined behavior: `Enum::from_raw(n) -> Option<Enum>` converts a number safely, and a `match` on an invalid value, read through a pointer or from C, panics (`invalid value 9 for enum Name`). C enumerations whose values combine as flags are written as integer constants instead. |
| Q480 | `std::mem` | `size_of<T>() -> usize` and `align_of<T>() -> usize` for any type; and `unsafe` `zeroed<T>() -> T` for FFI-safe types other than function pointers (those need `Option`) and `@repr` enums without a zero discriminant, as Rust's: the value out-parameters of C start from. Rejected types are compile errors. |
| Q481 | Taking addresses | `&raw const place` gives a `*const T`, and `&raw mut place` a `*mut T` (the place must be `var`), as in Rust. A place is a binding, a field of a place, an element of a fixed array place, or `*p`. Taking the address is safe; using it needs `unsafe` as any raw pointer does. A place holding ARC references cannot have its address taken (the rule of ADR-0026 for pointers). |
| Q482 | Lists as buffers | `list.as_ptr()` and `list.as_mut_ptr()` on `List<T>` for FFI-safe `T`: a pointer to the first element, valid until the list changes its length; and `list.resize(n, value)` as Rust's `Vec::resize`, to give C a buffer of `n` elements to fill. |
| Q483 | Reading bytes | `fs::read(path) -> Result<List<u8>, io::Error>`, as Rust's. Writing bytes waits for slices ([ADR-0079](0079-references-and-borrowing.md)). |
| Q484 | Dynamic libraries | `std::ffi::Library`: `Library::open(path) -> Result<Library, String>` (`LoadLibraryW`, `dlopen`), and `lib.symbol(name) -> Option<unsafe extern "C" fn()>`, converted with `as` inside `unsafe` (Q474). A library is never unloaded, so its function pointers stay valid until the program ends. |
| Q485 | Not in this record | Variadic functions, C++ and its name mangling (C++ libraries are used through their C interfaces), bitfields, packed structs, and a generator of bindings from C headers or `vk.xml`, which the separate project may write in Sagitario. |
| Q486 | Acceptance | A program without C files: sorts a list with `qsort` and a Sagitario comparison function; loads the C library of the system with `Library::open`, finds a function in it and calls it through a pointer; passes a `@repr(C)` struct with a fixed array, a union and a `@repr(i32)` enum to a C function and back; fills a buffer from C through `&raw mut` and `as_mut_ptr`; reads a binary file with `fs::read`; and passes with the sanitizers. |
| Q487 | Documents | This record, `docs/spec/10-ffi.md`, `docs/spec/02-types.md` (unions, enum representations, `List::resize`), `docs/spec/11-standard-library.md` (`std::mem`, `fs::read`, `std::ffi::Library`) and `docs/roadmap-v2.md`, updated by each increment. |

## Alternatives considered

- **`@c fn(A) -> R` or `*const fn(A) -> R` for function pointers:** Rust's form keeps one
  vocabulary with `extern "C" fn` items, and a pointer type would suggest `*f` and `f.add(n)`,
  which mean nothing for functions.
- **Nullable function pointers with `is_null()`:** a null pointer would be checked at every
  call; with `Option`, as in Rust and as ADR-0006 does for everything else, a callable pointer is
  never null.
- **C shims in `[link] sources`:** they work today, but each wrapper of a library would carry C,
  which is what this record removes.
- **Enums as raw integers only (`const` per value, as `ash` does):** loses exhaustive `match`;
  both remain possible, since constants still work for flags.
- **`&x` for addresses, as in C:** `&` is kept for the safe references of ADR-0079, as in Rust.
- **Unloading libraries when the last `Library` is released:** the function pointers taken
  from it would dangle; Rust's `libloading` needs lifetimes to prevent it, which Sagitario does
  not have.

## Consequences

- A package wrapping Vulkan, OpenGL, Win32, X11, Wayland or the Objective-C runtime is written
  in Sagitario alone; the separate project of the engine starts from that.
- The type checker gains function pointer types, unions and enum representations; the parser,
  `unsafe fn`, `union` and `&raw`; the runtime, the loading of libraries and `fs::read`.
- `union` and `raw` are contextual keywords, as in Rust (`union` before a name in an item,
  `raw` after `&`), so no existing name breaks; the VS Code grammar learns them.
