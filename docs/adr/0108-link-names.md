# 0108. Link names

- Status: Accepted
- Date: 2026-11-21
- Refines: [ADR-0026](0026-ffi-details.md) (the symbol of an imported function is its name),
  [ADR-0086](0086-c-globals-and-std-io.md) (Q558: the symbol of a C global is its name)

## Context

A function or global of an `extern "C"` block is named in Sagitario by its C symbol, which must be
a C identifier. Two needs do not fit.

**Apple's variants.** On macOS for Intel, Apple's C library keeps the symbols of 32-bit inodes,
`stat` and `readdir`, for programs built before 2008, and C's headers name the 64-bit ones,
`stat$INODE64` and `readdir$INODE64`, through assembler labels. `std::sys` declared the plain
symbols, with the old layouts of `struct stat` and `struct dirent`. AddressSanitizer intercepts
`readdir` assuming the 64-bit `struct dirent`, so it read the length of an entry at the wrong
offset and reported a heap-buffer-overflow of tens of kilobytes in every program listing a
directory (`tests/run-pass/file_metadata.sag` and `std_fs.sag`), which stopped the macOS jobs of
the release. `docs/technical-debt.md` recorded it as a bug of Apple's library and turned the macOS
release off; reproduced under Rosetta, the report comes from ASan's own interceptor and goes away
when C calls `readdir$INODE64`, as C's headers do. On both architectures, Apple's plain
`realpath` is also the one before POSIX 2008, and C's headers name `realpath$DARWIN_EXTSN`.

**Names of a program's own.** A program declaring a C function wants to call it by a name of its
own: two names for one symbol, a symbol that clashes with a function of the module, or one chosen
per target with `@cfg`, as a game engine does with the functions of each system. Rust has
`#[link_name = "symbol"]` for this.

The user chose an attribute as Rust's, with `$` allowed through an assembler label.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q770 | Syntax | `@link_name("symbol")` before a function or a global (`static`, `static var`) of an `extern "C"` block, as `@export("symbol")` before an `extern "C" fn`: once, with one string. Sagitario code uses the declared name; the linker sees the symbol. Elsewhere, or without one string, it is `E0105`. |
| Q771 | Symbols | The symbol follows the rules of C symbols (`E0208`): ASCII letters, digits and `_`, not starting with a digit, not reserved by C or by Sagitario; a link name may also hold `$` after its first character, for Apple's variants. A symbol declared with and without a link name is one symbol: it can be declared again only with the same signature, as any C function. |
| Q772 | C | A symbol that is a C identifier is declared in the FFI unit by its own name, which is strict C11. One holding `$`, which C cannot name, is declared under a name of the FFI unit (`sag_link_N`) with an assembler label naming the symbol, `__asm__(SAG_LINK_NAME("readdir$INODE64"))`, with the platform's prefix of C symbols (`__USER_LABEL_PREFIX__`, `_` on Apple's systems). clang and gcc accept these labels with `-pedantic-errors`; with any other compiler (MSVC `cl`) the FFI unit stops with `#error`. Only Apple's systems need such symbols. |
| Q773 | `std::sys` | On macOS for Intel, `std::sys` declares `stat`, `lstat`, `fstat`, `opendir` and `readdir` with the `$INODE64` symbols, whose layouts are those of Apple silicon, and on Apple's systems `realpath` as `realpath$DARWIN_EXTSN`: the symbols C's headers name. The offsets of macOS for Intel go, and it gains the time of birth of files. |
| Q774 | CI | The release builds, tests with sanitizers and installs on macOS again, on both architectures, and `docs/technical-debt.md` drops the entry. |

## Alternatives considered

- **`readdir_r`, `getdirentries64`, `fts`, or `opendir$INODE64` without a way to name it** (in
  `docs/technical-debt.md`): deprecated, private, built on `readdir`, or impossible to write.
- **Sanitizers off on macOS, or no macOS release**: hides the fault instead of fixing it, against
  the rule of not loosening checks; ASan was right that the layouts differed.
- **A C shim in the runtime calling `readdir` through `<dirent.h>`**: S1 removed the C of the
  standard library, and a program declaring Apple's variants would still need it.
- **`$` written in a C identifier**: clang and gcc accept it as an extension, which
  `-pedantic-errors` rejects.
- **Only C identifiers**: does not reach Apple's variants, the reason for the record.

## Consequences

- Programs and bindings call C functions by names of their own, and Apple's variants are
  reachable. `bindgen` keeps writing the names of the headers.
- macOS for Intel is checked with sanitizers again; locally, under Rosetta with
  `SAG_CC="clang -arch x86_64"` (a wrapper script) and `arch -x86_64`.
- The seed is updated, since the standard library uses the attribute.
