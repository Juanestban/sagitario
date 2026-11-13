# 0101. The rest of S2: numbers, ranges, pointers, paths, hashing, operators and formats

- Status: Accepted
- Date: 2026-11-13
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: the acceptance of
  S2), [ADR-0081](0081-operator-overloading.md) (the bitwise operators were not overloadable),
  [ADR-0089](0089-display-debug-and-format-specifiers.md) (widths and precisions were literal
  numbers; the debug form of an interface value was `dyn Name`), [ADR-0088](0088-bitwise-operators-and-integer-methods.md),
  docs/spec/02-types.md, 05-interfaces-generics.md, 07-errors.md, 11-standard-library.md

## Context

The ten records of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) are done, and the
benchmarks run with their results recorded ([`docs/benchmarks.md`](../benchmarks.md)). S2 is
accepted when every row of the audit is ✅, ➖ or ⛔ with its reason (Q542). Twelve rows are not:

- **Numbers:** the conversions of integers to and from bytes (`to_le_bytes`, `from_be_bytes`...),
  and parsing in a radix (`from_str_radix`).
- **`Duration`:** `checked_add`, `saturating_sub`, `mul_f64` and the operators Rust has.
- **Ranges:** values with `contains` and `is_empty`, not only iterators.
- **Raw memory:** the named functions of `std::ptr` (`null`, `read`, `write`, `copy`...).
- **Paths:** `is_absolute`, `with_extension`, `strip_prefix` and the components of a path.
- **Hashing:** a `Hasher` to combine the hashes of fields.
- **Operators:** the bitwise interfaces (`BitAnd`...) and `*Assign`.
- **Formats:** widths and precisions given at run time, and the debug form of an interface value.
- **Errors:** `source()` chains and backtraces.
- **`os`:** the extensions of each system.

The user chose, for the four that change the language or the existing API: a `Hasher` beside
`hash() -> u64`; the bitwise interfaces, superseding ADR-0081's rule; Rust's `name$` widths and
precisions, with the debug form of an interface value's type; and `source()` now, backtraces in S3.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q702 | Integers and bytes | Every integer type has `to_le_bytes()`, `to_be_bytes()` and `to_ne_bytes()`, which give `[u8; N]` with `N` its size (`usize` and `isize` the target's), and the static `from_le_bytes(bytes)`, `from_be_bytes` and `from_ne_bytes`, as Rust's. The native order is the target's, little-endian on every supported system. |
| Q703 | Parsing in a radix | `i32::from_str_radix(text, radix) -> Result<i32, String>` for every integer type, as Rust's, with the messages of `parse_i32` (`invalid digit found in string`...): an optional sign (`-` only for signed types, and `+` for any), then digits of the radix, upper or lower case. A radix outside 2 to 36 panics, as Rust's. |
| Q704 | `Duration` | `checked_add`, `checked_sub`, `checked_mul(u32)` and `checked_div(u32)` give `Option<Duration>`; `saturating_add`, `saturating_sub` and `saturating_mul`; `mul_f64`, `div_f64`, `from_secs_f64` and `as_secs_f32`, `from_secs_f32`, `mul_f32`, `div_f32`; and the operators `+`, `-`, `*` and `/` by a `u32`, implementing `Add`, `Sub`, `Mul<u32, Duration>` and `Div<u32, Duration>`, which panic on overflow as Rust's. `plus`, `minus` and `times` stay. `Duration` has no `Display`, as in Rust; its debug form stays a struct's. |
| Q705 | Ranges | `contains(value)` and `is_empty()` on a range written directly, as `rev()` is (`(1..=9).contains(x)`), with Rust's meaning for `a..b` and `a..=b`. A range stays an iterator otherwise; `start` and `end` are its bounds as written. |
| Q706 | `std::ptr` | A module `std::ptr` with `null<T>() -> *const T` and `null_mut<T>() -> *mut T`, whose `T` comes from the expected type, and, inside `unsafe`, `read(src: *const T) -> T`, `write(dst: *mut T, value: T)`, `copy(src, dst, count)` (the regions may overlap), `copy_nonoverlapping(src, dst, count)` and `write_bytes(dst, byte, count)`, as Rust's, on C's `memmove`, `memcpy` and `memset` for `count` values of `T`. |
| Q707 | Paths | `std::path` keeps paths as `String` (`PathBuf` is ➖: a path is a text). It gains `is_absolute(path)`, `is_relative(path)`, `has_root(path)`, `with_extension(path, extension)`, `with_file_name(path, name)`, `strip_prefix(path, base) -> Option<String>`, `starts_with(path, base)` and `ends_with(path, child)` by components, and `components(path) -> List<Component>`, with `Component` as Rust's: `Prefix(String)` (`C:` on Windows), `RootDir`, `CurDir`, `ParentDir` and `Normal(String)`. Separators are each system's, as `join`'s. Normalizing a path is `fs::canonicalize`, as in Rust. |
| Q708 | `Hasher` | A module `std::hash` with `interface Hasher { fn write(self, bytes: &[u8]); fn finish(self) -> u64; }` and its defaults `write_u8` to `write_u64`, `write_usize`, `write_i8` to `write_i64`, `write_isize` and `write_str`, and `DefaultHasher`, Rust's SipHash-1-3 with zero keys, `new DefaultHasher()`. `Hash` stays `fn hash(self) -> u64`: a type combines its fields with `hasher.write_u64(field.hash())`, then `finish()`. |
| Q709 | Bitwise operators | The prelude's `BitAnd<Rhs, Out>`, `BitOr`, `BitXor`, `Shl<Rhs, Out>` and `Shr<Rhs, Out>`, and `Not<Out>` for the prefix `!`, as `Add<Rhs, Out>` and `Neg<Out>` (ADR-0081), superseding its rule that they are not overloadable; `a &= b` and the others are `a = a & b`, as `+=` is. Integers and `bool` keep their own operators. `&&` and `\|\|` stay not overloadable, as in Rust. |
| Q710 | Compound assignment | `AddAssign` and the other `*Assign` interfaces are ➖: `a += b` is `a = a + b` for every type implementing the operator (spec 05), the same result without a second interface. |
| Q711 | Widths and precisions at run time | In a format string, `{value:>width$}`, `{value:.precision$}` and `{value:width$.precision$}` take the width or precision from a binding of type `usize` named before `$`, as Rust 2021's inline arguments; a binding of another type is an error (`E0007`). Fill, alignment, sign and the other parts of the specifier are as for a literal width. |
| Q712 | The debug form of an interface value | `{value:?}` of an interface value writes the debug form of the value's own type, through its itable, as Rust's `dyn Debug` does, instead of `dyn Name`. |
| Q713 | `Error::source` | The prelude's `Error` gains `fn source(self) -> Option<Error>`, a default method giving `None`, as Rust's: an error caused by another overrides it to give it, and code follows the chain. `io::Error` gives the error it was made from with `io::Error::other(error)`, as Rust's. |
| Q714 | Backtraces | ⛔ until S3: a backtrace walks the stack and reads the symbols of each system from the runtime's core, which S3 rewrites in Sagitario. |
| Q715 | `os` | ➖: the descriptors and `HANDLE`s of `File` and the sockets are reached with `as_raw_handle` and `from_raw_handle` on every system, and the rest of each system (the permissions of Unix, the handles of Windows) through `extern "C"`, which Sagitario's FFI reaches without the traits Rust needs. |
| Q716 | `@cfg` on fields | ⛔ for now, as the audit says: no class of `std::sys` needs it more than the two definitions it has; a record of its own when one does. |
| Q717 | The editor | `sag lsp` completes the new methods, functions and types as any of `std`, and the interfaces of Q709 as those of the prelude; the grammar colors `name$` in format specifiers. |
| Q718 | Increments | **1** Integers and bytes, parsing in a radix, `Duration` (Q702–Q704). **2** Ranges, `std::ptr` and paths (Q705–Q707). **3** `Hasher` and `Error::source` (Q708, Q713). **4** The bitwise interfaces (Q709). **5** Widths and precisions at run time and the debug form of interface values (Q711, Q712). Each with its tests, the spec, the editor and the rows of the audit; with the last, every row of the audit is ✅, ➖ or ⛔, and S2 is accepted. |

## Alternatives considered

- **Rust's `Hash`** (`fn hash(self, state: Hasher)`): Rust's API, but every implementation, those
  of `std` included, would change, and the compiler's hashing of tuples and of `HashMap` with it.
- **The bitwise operators of integers only** (ADR-0081): no change, but bit sets and masks of
  user types cannot be written `a | b`.
- **Widths and precisions as literals only:** a variable width would need `pad_start` and
  `repeat`, which Rust's `{:>width$}` writes in the format string.
- **Backtraces in the C core now:** they would be rewritten in S3, with the core.

## Consequences

- With its five increments, every row of the audit of S2 is ✅, ➖ or ⛔, and S2 is accepted.
- The prelude gains six operator interfaces and `Error::source`; existing programs keep working.
- Format strings gain `name$` in their specifiers, and interface values their own debug form.

## Notes

- Increment 1 is done. The conversions to bytes and `from_str_radix` are functions of the prelude
  for each integer type, made from templates by the loader as the range functions are
  (`int_to_le_bytes_u32` and the rest), those of `usize` and `isize` twice, under
  `@cfg(target_pointer_width)`, so that their arrays have the target's size. The compiler calls
  them for the methods and the static functions (`u32::from_le_bytes`). `Duration`'s operators
  check their arithmetic, so that they panic in release builds too, as Rust's; its `nanos` is a
  `u64`, so the longest duration is about 584 years, where Rust's is far longer.

