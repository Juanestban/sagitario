# 0103. Build scripts and binding generation (S4)

- Status: Accepted
- Date: 2026-11-14
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q544: S4),
  [ADR-0102](0102-binding-generation-before-the-core.md) (S4 before S3),
  [ADR-0078](0078-ffi-without-c-shims.md) and [ADR-0026](0026-ffi-details.md) (variadic functions
  and opaque types were not supported), [ADR-0082](0082-format-strings.md) and
  [ADR-0093](0093-everyday-methods.md) (Sagitario had no macros), [ADR-0041](0041-dependencies-by-path.md),
  docs/spec/01-lexical.md, 09-modules.md, 10-ffi.md, docs/nova.md

## Context

S4 is a tool that writes Sagitario declarations from C headers, as Rust's `bindgen` does (Q544).
Its acceptance: bindings generated from the headers of a real C library are used by a program in
the harness without being edited by hand. Its record settles the inputs, the translation of
macros, opaque types and callbacks, and the command.

In Rust, bindgen is a crate, used mostly from a package's build script (`build.rs`). Cargo
compiles and runs the script before the package. The script writes the bindings to `OUT_DIR`,
and the package reads them with `include!(concat!(env!("OUT_DIR"), "/bindings.rs"))`. The script
also tells Cargo, on its output, which libraries to link. `bindgen-cli` does the same from the
command line, once.

Sagitario has none of that. Nova has no build step, and `[link]` names libraries statically. The
language has no macros: each file is a module, with no `mod` and no `include!`. The FFI cannot
call variadic functions, so the runtime has C helpers only to call `fcntl`
(`sag_socket_nonblocking`, `sag_socket_duplicate`). It cannot name an opaque C type
(`typedef struct SDL_Window SDL_Window;`) except as `c_void`.

The user chose:

- Rust's model: a build script `build.sag`, as Cargo's, and Rust's `include!` to read what it
  writes.
- bindgen as a library the script uses, plus a command.
- C headers as the input, preprocessed by the C compiler the toolchain already has.
- Opaque types as stable bindgen writes them.
- Calls of variadic functions, as Rust allows.

## Decision

### The FFI

| # | Topic | Decision |
|---|---|---|
| Q720 | Variadic functions | A function in an `extern "C"` block may end its parameters with `...`, after at least one: `fn SDL_Log(fmt: *const c_char, ...);`, as Rust's. It is only called, inside `unsafe`, and each extra argument must be FFI-safe and of a type C does not promote, also as in Rust: `i8`, `i16`, `u8`, `u16`, `bool` and `f32` are errors that suggest the cast (`as c_int`, `as f64`). Sagitario code cannot define a variadic function, and a variadic function is no function pointer. |
| Q721 | Opaque types | A `@repr(C)` struct whose only field is a `prv` array of length 0 (`@repr(C) struct SDL_Window { prv _unused: [u8; 0]; }`, as stable bindgen writes it) is opaque. It can only be used behind a raw pointer, so its values never exist. A local, a field, a parameter or a result of its type, `*ptr` of a pointer to it, and `size_of` or `align_of` of it are errors. Its C is an incomplete struct, since C11 has no arrays of length 0. An array of length 0 is an error anywhere else. `*mut SDL_Window` and `*mut SDL_Renderer` are different types. |
| Q722 | The runtime's helpers | `std::sys` calls `fcntl` as a variadic function, and the C helpers `sag_socket_nonblocking` and `sag_socket_duplicate` are deleted. The sockets of Apple systems get `FD_CLOEXEC`, which they lacked for want of `fcntl`. |

### Built-in macros

| # | Topic | Decision |
|---|---|---|
| Q723 | Macros | Sagitario gains Rust's built-in macros that read files and the environment when the program is compiled: `include!`, `include_str!`, `include_bytes!`, `concat!` and `env!`. A macro is called as `name!(...)`. Its name is a path with no root (`include!`, never `std::include!`). Programs cannot define macros: `macro_rules!` and procedural macros would need a record of their own. Any other `name!(...)` is an error. |
| Q724 | `include!` | `include!(path)`, written where a module-level item may be, puts the items of the file at `path` in the current module, as if written there. Their spans are the file's, and its errors are reported in it. `path` is a string literal or a `concat!` of them and of `env!`. A relative path is relative to the directory of the file calling it. The file may not call `include!` itself, so there are no cycles. A missing file is an error. Rust also allows `include!` as an expression; that is left out. |
| Q725 | `include_str!`, `include_bytes!` | `include_str!(path)` is an expression of type `String` with the text of the file, which must be UTF-8. `include_bytes!(path)` is a `[u8; N]` with its bytes. Both are constants, and both resolve `path` as `include!` does. |
| Q726 | `concat!`, `env!` | `concat!(a, b, ...)` joins literals (strings, integers, floats, `char`, `bool`) and the macros among them into one string literal. `env!("NAME")` is the value, as a string literal, of the environment variable `NAME` of the compiler. A variable that is not set is an error, with the help to set it. `env!("NAME", "message")` gives its own message. Both are evaluated before the passes, wherever a string literal may be written. |

### Build scripts

| # | Topic | Decision |
|---|---|---|
| Q727 | `build.sag` | A package whose directory has `build.sag` next to `Nova.toml` has a build script, as Cargo's `build.rs`. Before compiling the package, Nova compiles the script for the host machine (with the same `--release` and C compiler) and runs it in the package's directory. The script sees `std`, the library `bindgen` (Q731) and no dependency of the package. A script that fails (a compile error, a panic or a non-zero exit code) fails the build, with its output shown. The scripts of a package's dependencies run before it. `nova check` runs them too, since the package may include what they write. |
| Q728 | Its environment | Nova runs the script with `OUT_DIR` (`target/<profile>/build/<package>/out`, created empty the first time), `TARGET` (the target triple of the build), `HOST`, `PROFILE` (`debug` or `release`), `NOVA_MANIFEST_DIR` (the package's directory), `NOVA_PKG_NAME`, `NOVA_PKG_VERSION`, and `SAG_CC` (the C compiler of the build). Nova sets `OUT_DIR` again when it compiles the package, so that `env!("OUT_DIR")` reads it. |
| Q729 | Its directives | The lines of the script's output that start with `nova::` are directives, as Cargo's `cargo::`. `nova::link-lib=NAME` links a library, `nova::link-search=PATH` adds a search path, and `nova::link-source=PATH` compiles and links a C source, as `[link]` does (`libraries`, `search_paths`, `sources`). `nova::rerun-if-changed=PATH` and `nova::rerun-if-env-changed=NAME` say when the script must run again. `nova::warning=MESSAGE` shows a warning. Any other `nova::` line is an error; other lines are only kept in `target/<profile>/build/<package>/output`. |
| Q730 | When it runs again | Nova runs the script again when the script, the `Nova.toml` or `SAG_CC` change, or when no output of a previous run exists. With `rerun-if-changed` or `rerun-if-env-changed`, it also runs again when one of those paths (a file or, recursively, a directory) or variables changes. Without them, any file of the package changing does, as in Cargo. `nova clean` removes the outputs. |

### bindgen

| # | Topic | Decision |
|---|---|---|
| Q731 | The library | `bindgen` is a library Sagitario ships beside `std`, a path root only build scripts see, as the crate is in Rust. `bindgen::Builder::new().header("wrapper.h").allow_prefix("SDL_").include_path("vendor/include").generate()` gives `Result<Bindings, bindgen::Error>`, and `bindings.write_to_file(path)` writes them: into the directory `OUT_DIR` names, which a script reads from its environment at run time with `std::env`, as `build.rs` does. Other options: `allow_file(path)`, `block(name)`, `define(name, value)` and `clang_arg(arg)`. Its API is settled in its increment and listed in the spec, as `std`'s. |
| Q732 | The command | `nova bindgen <header> [--output <file>] [--allow-prefix <prefix>] [--include-path <dir>] [--define <name>=<value>] [--target <target>]` writes the same bindings once, as `bindgen-cli`, to `<file>` or to standard output. It needs no package. |
| Q733 | Its input | C headers. bindgen runs the C compiler of the build on them, `clang -E` for the target's triple, with the include paths and definitions given, and reads the result with a parser of the declarations C headers use. Its line markers tell bindgen which file each declaration comes from. `clang -dM -E` gives the macros. No dependency is added, since the C compiler is already a requirement. C++ headers and descriptions such as Vulkan's `vk.xml` are left out. Vulkan's C headers work. |
| Q734 | What it writes | Functions become `extern "C"` declarations, variadic ones included (Q720). `struct` and `union` become `@repr(C)` structs and unions with `pub` fields. Function pointers become `Option<unsafe extern "C" fn(..) -> R>`, since C may give `NULL`, as bindgen writes them. Arrays become `[T; N]`, `const T *` becomes `*const T`, and C's integer types become `std::ffi`'s (`c_int`, `c_ulong`...). `bool` stays `bool`, and fixed-width types (`uint32_t`) become `u32`. A declared but undefined struct, and a struct used only behind pointers whose definition is not allowed, becomes opaque (Q721). Sagitario has no type aliases, so a `typedef` is replaced by the type it names, and a `typedef` of a struct names the struct. |
| Q735 | Enums | A C `enum` becomes integer constants, as bindgen's default, since C's enumerations often combine as flags: `pub const SDL_INIT_VIDEO: u32 = 32;`. Its constants and the places that use the enum get its underlying type for the target, as clang gives it. A Sagitario `@repr` enum would panic on a value that is no variant ([spec 10](../spec/10-ffi.md#enums-with-an-integer-representation)). |
| Q736 | Macros of C | A macro without parameters whose body is a constant expression (integer, float, string or `char` literals, other such macros, casts to integer types, and C's arithmetic, bitwise and shift operators) becomes a `pub const` of its type, as bindgen does. `#define SDL_INIT_VIDEO 0x00000020u` gives `pub const SDL_INIT_VIDEO: u32 = 32;`. Macros with parameters, and the others, are skipped, as bindgen does. |
| Q737 | Choosing and naming | The declarations of the header given, and of the headers it includes from its own directory, are written. With `allow_prefix` or `allow_file`, only those whose names or files match are written, and then the types they use. `block` leaves one out. A name that is a Sagitario keyword gets `_` at its end (`type_`), as bindgen does. A name Sagitario cannot declare, or a declaration bindgen cannot translate, is skipped with a comment in the output saying why. Such declarations are bitfields, packed structs, `static inline` functions with no symbol, `long double`, and K&R declarations. |

### Increments and acceptance

| # | Topic | Decision |
|---|---|---|
| Q738 | Acceptance | `tests/nova/bindgen_*` packages of the harness: a C library of the harness (a header and a C source) whose header has every construct of Q734–Q737. Its `build.sag` generates its bindings with `bindgen`, compiles its source with `nova::link-source`, and `main.sag` uses it through `include!`. Then the bindings of the system's C library (`<string.h>`, `<time.h>`) on every platform of the CI. Neither is edited by hand. |
| Q739 | Increments | 1: variadic calls, opaque structs and the runtime's helpers (Q720–Q722). 2: the built-in macros (Q723–Q726). 3: build scripts (Q727–Q730). 4: bindgen's parser, its types and declarations (Q731, Q733–Q735, Q737). 5: the macros of C, the command and the acceptance (Q732, Q736, Q738). Each with its spec, tests and editor support (the grammar of `name!`, `...` and `build.sag`, completion of the macros and of the `nova::` directives). |

## Alternatives considered

- **A declarative `[bindings]` table in `Nova.toml`, without build scripts:** smaller, but every
  need beyond bindgen's options (a generated table, a C library built with its own steps) would
  need another table, and Rust's packages are written with `build.rs`.
- **A path root `out` for the files of `OUT_DIR`, instead of `include!`:** it needs no macros, but
  it is not Rust's form, and the user chose `include!`.
- **`macro_rules!` now:** a macro system of its own, not needed for S4.
- **Nova's own C preprocessor:** it would work without a C compiler, but it would have to repeat
  the predefined macros of each compiler and system.
- **`*mut c_void` for every opaque type:** no change to the language, but any pointer could be
  passed where another goes.
- **`extern "C" { type SDL_Window; }`, Rust's extern types:** more exact than a struct of length
  0, but only on Rust's nightly, and new syntax.
- **C enums as `@repr` enums:** a flag combination, or a value a newer library adds, would panic.
  bindgen's `rustified_enum` can be added later as an option.
- **Bitfields and packed structs:** they need the record layout of the C compiler for each target
  (`-fdump-record-layouts`). They are left for later, as in spec 10, and skipped with a comment.

## Consequences

- Sagitario has its first macros, built in: `name!(...)` is new syntax. ADR-0082's reason for
  `f"..."` stands, since `println` takes no format.
- Nova runs code of the package when it builds it, as Cargo does: a package's build script runs
  with the user's permissions.
- The harness gains packages that run `clang -E` through bindgen. They run wherever the C
  compiler is the toolchain's, with cross-compilation through `--target`.
- The spec gains the macros (01, 09), variadic calls and opaque structs (10), and the library
  `bindgen`. `docs/nova.md` gains build scripts and `nova bindgen`.
- The roadmap's S4 is done with this record.

## Notes

- Increment 1 is done. `...` is a token of its own. Each call of a variadic function with other
  types of extra arguments gets a C wrapper of its own, since a wrapper cannot forward them; a call
  without extra arguments uses the function's. Q721's "an array of length 0 is an error anywhere
  else" applies to the fields of `@repr(C)` structs and unions, where C has no such arrays: other
  arrays of length 0 stay valid, as they were (C gets one element that is never read). Generic
  code given an opaque type that holds a value of it, as `ptr::read`, is found when its instance
  is lowered, so `sag check` does not report it. As with `c_void`, an opaque type is not a type
  argument written explicitly (`ptr::null_mut<SDL_Window>()`); one inferred from a pointer is.
- `include_str("path")` already exists, without `!` ([ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md),
  Q222), and the compiler uses it to embed the runtime and `std`, which the seed compiles. Q725's
  `include_str!` meets it in increment 2.
- Increment 2 is done. The parser expands the macros of expressions into literals as it reads
  them, and keeps `include!` for the loader, which parses the file and adds its imports and items
  to the module. `include_bytes!` gives an array literal of `u8`s, `[0u8; 0]` for an empty file.
  Paths given by `concat!` and `env!` may be absolute. `include_str!` replaces the
  `include_str(...)` of ADR-0049, by [ADR-0104](0104-include-str-as-a-macro.md): the seed was
  updated so that the compiler's sources could use it, which showed that the seed, one file of C,
  no longer compiled once the standard library exported functions to the runtime; it now keeps
  the units of the FFI apart.

