# 10. Foreign function interface (F1)

The C ABI is how Sagitario talks to operating systems and how mobile hosts (Swift, Kotlin
via JNI) will call Sagitario code.

## Importing C functions

```sag
extern "C" {
  fn write(fd: c_int, buf: *const u8, len: usize) -> isize;
}

fn main() {
  unsafe { write(1, "hello".as_ptr(), 5); }
}
```

- Functions declared in `extern "C" { ... }` can only be called inside an `unsafe` block.

## Exporting functions

```sag
@export("sag_add")
extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
```

- `extern "C" fn` gives a function the C ABI; `@export("symbol")` sets its exported symbol
  name.
- Exported functions allow building Sagitario as a static or dynamic library (build modes
  beyond executables are F3).

## `unsafe`

- Raw pointers `*const T` and `*mut T` may be declared and passed anywhere, but can only be
  **dereferenced** inside `unsafe`.
- Manual retain/release of class references is only possible inside `unsafe`.

## C-compatible types

- The `ffi` module provides C types: `c_int`, `c_char`, `CStr`, etc.
- `@repr(C)` gives a struct C layout:

```sag
@repr(C)
struct Timespec {
  pub tv_sec: i64;
  pub tv_nsec: i64;
}
```
- Class references do not cross the FFI boundary directly. They are passed as opaque handles,
  with explicit retain/release inside `unsafe`.

## Post-MVP

Automatic binding generation (in the style of `bindgen`).
