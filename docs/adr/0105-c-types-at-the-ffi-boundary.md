# 0105. C's own types at the FFI boundary, for callbacks

- Status: Accepted
- Date: 2026-11-14
- Refines: [ADR-0078](0078-ffi-without-c-shims.md) (C function pointers),
  [ADR-0103](0103-build-scripts-and-binding-generation.md) (the callbacks left open by its
  notes), [ADR-0104](0104-include-str-as-a-macro.md) (the bootstrap's `-fno-builtin`),
  docs/spec/10-ffi.md, docs/architecture.md

## Context

A C library calls a program's `extern "C" fn` through a function pointer of its own types. The
acceptance of S4 ([ADR-0103](0103-build-scripts-and-binding-generation.md)) showed that
`--sanitize` reports such a call when the callback takes a pointer to a struct: C calls
`int (*)(const fx_rect *)`, and the program's function is `int sag_fn_1(sag_s3 const *)`.

C11 makes a call through a pointer of an incompatible function type undefined (6.3.2.3p8), and
two types of different translation units are compatible only by C's rules (6.2.7): the same tag
and members for structs, and the same integer types. `-fsanitize=function` checks them. An
experiment with clang 23 on Windows, a callback defined in one unit and called from another:

| The callback's parameter, and the caller's | Reported |
|---|---|
| `struct rect *` and `struct rect *` of the other unit | no |
| `struct anon_t *` and the other unit's `typedef struct { ... } anon_t` | no |
| `struct other *` and `struct rect *`, the same layout | yes |
| `int8_t *` and `char *` | yes |
| `int32_t` and `long` (Windows, where both have 32 bits) | yes |

So the C the compiler generates is the problem, not the layouts: it names every aggregate
`sag_sN`, and writes `std::ffi`'s types as the fixed-width types they are in Sagitario (`c_char`
as `int8_t`, `c_long` as `int32_t` on Windows), which are other types for C. Callbacks of real
libraries take such types all the time: SDL's take `SDL_Event *`, `const char *` and `void *`.

Two units of C come from each program ([architecture](../architecture.md)). The program's
includes the runtime's header, which includes `<math.h>` and so the tags of the system's C
library (`struct _exception` with MSVC); the FFI unit, `sag_ffi.c`, includes only `<stdint.h>`,
`<stddef.h>` and `<stdbool.h>`, and declares the C functions the program calls, with wrappers
the program calls.

The user chose to write C's own types in the FFI unit, and, when two types of a program have the
same name, to give it to the first.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q742 | C's integer types | In the FFI unit, the types of `std::ffi` are written as C's: `c_char` as `char`, `c_schar` as `signed char`, `c_uchar` as `unsigned char`, `c_short`, `c_ushort`, `c_int`, `c_uint`, `c_long`, `c_ulong`, `c_longlong` and `c_ulonglong` as `short` to `unsigned long long`, and `c_float` and `c_double` as `float` and `double`. In Sagitario they stay what they are (`c_char` is an `i8` or a `u8`, `c_long` an `i32` or an `i64`): the compiler keeps, with the types of the FFI's signatures and of the fields of `@repr(C)` types, the name of `std::ffi` they were written with. Other integer types stay `int32_t` and the like, which are C's `int` and the like. |
| Q743 | C's tags | In the FFI unit, a `@repr(C)` struct, union or opaque type is `struct Name` or `union Name`, with its Sagitario name, as `bindgen` gives the tag or the typedef of C ([ADR-0103](0103-build-scripts-and-binding-generation.md), Q734); an opaque type is only declared. A name that cannot be a tag keeps the generated one: a C keyword, a name C reserves (`__x`, `_X`), or one of the runtime's (`sag_`). |
| Q744 | Callbacks | An `extern "C" fn` of the program used as a value is, for C, a trampoline of the FFI unit: a function with the C types of Q742 and Q743 that calls the Sagitario function directly. Its address is the value, which Sagitario code calls too, so that a function pointer is the same pointer everywhere. The functions `@export` defines are written with those types as well. |
| Q745 | Names taken twice | When two `@repr(C)` types of a program have the same name (two packages with the bindings of the same library), the first in the order of the program's modules gets it as its tag, and the others keep generated ones. A callback C calls with one of those may still be reported by `--sanitize`, which the spec says. |
| Q746 | The program's unit | The program's unit keeps its names (`sag_sN`, `int32_t`), since its header includes the system's tags. Values cross between the two units as they do today, through the wrappers of the FFI unit, with the same layouts: direct calls, which have the C ABI of both and which no sanitizer checks. |
| Q747 | The C library's declarations | The FFI unit's declarations of the C library's functions use the same types, so that they agree with the C compiler's own declarations of them: `std::sys` declares its buffers as `*const c_void` and its strings as `*const c_char` where C does (`fwrite`, `fread`, `strerror`), and the bootstrap compiles the seed's FFI units without `-fno-builtin` ([ADR-0104](0104-include-str-as-a-macro.md)). |
| Q748 | Tests | The fixture of S4 takes again `const fx_rect *` and `const char *` in its callbacks, and gains one with `long`, all under `--sanitize`; a `run-pass` test calls back through `qsort` with a `@repr(C)` struct; the dumps of codegen show the trampolines. |
| Q749 | Increments | One. |

## Alternatives considered

- **`-fno-sanitize=function` for programs that pass callbacks to C:** much less work, and the
  calls are the same in the ABI, but it turns a sanitizer off, and the C stays incompatible by
  the standard.
- **Only documenting it:** callbacks would take `void *` and integers, and the program would cast
  them, as the fixture does now; SDL's callbacks would all need it.
- **C's names in the program's unit too:** its header includes `<math.h>`, whose tags a binding of
  the same names would define again; the FFI unit includes no header of the system.
- **An error for a name taken twice:** stricter, but two packages with the bindings of the same
  library could not be used together.

## Consequences

- Callbacks C calls with its own types, also through `const char *`, `long` and pointers to its
  structs, are compatible C, and `--sanitize` checks them as any call.
- The FFI unit and the program's name the same aggregates differently; the architecture says how.
- Programs see no change: the types of `std::ffi` stay Sagitario's integer types.

## Notes

- Implemented in one increment. A trampoline is C's type for callbacks only if Sagitario calls
  through pointers with C's types too: a call from the program's unit through its own type to a
  trampoline defined with C's would be reported the other way round. So every call through a C
  function pointer goes to a helper of the FFI unit, `sag_call_N`, which calls through C's type,
  and a C function of an `extern` block used as a value is the function itself rather than its
  wrapper. This completes Q744.
- Q747 holds for `strerror`, now `*mut c_char` as C returns it, and `std::sys` declares the
  buffers of `fwrite` and `fread` as `c_void`. But clang asks for `<stdio.h>` to declare any
  function taking a `FILE`, whatever its types, so the bootstrap keeps `-fno-builtin` for the
  seed's FFI units, with that reason in its comment.
- The fixture of S4 takes `const fx_rect *` again in its callback and gains `fx_log`, which calls
  back with `const char *` and `long`; `tests/run-pass/c_callbacks_with_c_types.sag` sorts
  `@repr(C)` structs with `qsort` and calls through pointers to `strlen` and to a callback.

