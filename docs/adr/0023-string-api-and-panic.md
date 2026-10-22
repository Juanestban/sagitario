# 0023. The `String` API, `StringBuilder` and `panic`

- Status: Accepted
- Date: 2026-10-22
- Amends: [ADR-0019](0019-prelude-and-printing.md) (the prelude gains `panic` and `StringBuilder`)

## Context

F1 increment 6 needs the `String` API, `StringBuilder`, `chars()` and `panic`. The specification
only said that strings are immutable UTF-8 with ARC, that `+`, templates and `StringBuilder`
build them, and that `chars()`/`bytes()` replace byte indexing (question Q22). Lists and
iterators, which most string APIs return, only arrive in F2.

## Decision

| # | Topic | Decision |
|---|---|---|
| 1 | `panic` | Prelude function `panic(message: String)` of the never type; prints `panic at file:line: message` and exits with code 101. |
| 2 | Operators | `+` concatenates and `+=` appends (creating a new `String`); `==`, `!=`, `<`, `<=`, `>`, `>=` compare bytes lexicographically, as Rust. |
| 3 | String patterns | String literals are patterns in `match`. |
| 4 | Methods | `len()` (bytes), `is_empty()`, `contains`, `starts_with`, `ends_with`, `find -> Option<usize>`, `trim`, `trim_start`, `trim_end`, `to_ascii_uppercase`, `to_ascii_lowercase`, `replace(from, to)`, `repeat(n)`, `substring(start, end)`. |
| 5 | Indices | Byte offsets; an index outside the string or inside a UTF-8 character panics. |
| 6 | Case | ASCII only in the MVP; Unicode case mapping is post-MVP. |
| 7 | `chars()` / `bytes()` | Only as the iterable of `for` in F1 (`char` and `u8` elements); real iterators in F2. |
| 8 | `char` methods | `is_ascii_digit`, `is_ascii_alphabetic`, `is_ascii_alphanumeric`, `is_ascii_whitespace`, `is_ascii_uppercase`, `is_ascii_lowercase`, `to_ascii_uppercase`, `to_ascii_lowercase`, `to_digit(radix) -> Option<u32>`. |
| 9 | Parsing | `parse_<type>()` for every integer type, `f32` and `f64`, returning `Result<T, String>` with Rust's messages; a `ParseError` type waits for the `Error` interface (F2). |
| 10 | `StringBuilder` | A reference type with ARC (a standard class from F2), created with `new StringBuilder()`, the only `new` in F1. Methods `append`, `append_char`, `len`, `is_empty`, `clear`, `to_string`; usable through `const` bindings. |
| 11 | Out of scope | `split`/`lines` (need `List` or iterators), `to_string()` on primitives (templates cover it), formatting macros. |

## Alternatives considered

- **Rust slices (`&s[a..b]`)**: slices are post-MVP; `substring` returns a `String` sharing the
  buffer, which is cheap since strings are immutable.
- **`StringBuilder` as a value type**: copying a builder would copy or share its buffer
  silently; as a reference it already behaves as the F2 class will.
- **`StringBuilder::new()`**: would break when it becomes a class, since classes are only built
  with `new`.

## Consequences

- The prelude is `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `String`, `StringBuilder`,
  `print`, `println`, `eprintln` and `panic`.
- The runtime gains the string, parsing, `char` and builder functions; `trim` and `substring`
  return views that share (and retain) the original buffer.
- `for` over `chars()`/`bytes()` iterates a copy of the string taken before the loop.
