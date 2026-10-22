# 1. Lexical structure

## Source files

- Source files use the `.sag` extension and must be valid UTF-8.
- Statements end with `;`. There is no automatic semicolon insertion.
- Blocks are delimited by `{` and `}`.

## Comments (F1)

| Form | Meaning |
|---|---|
| `// text` | Line comment |
| `/* text */` | Block comment; block comments **nest** |
| `/// text` | Documentation for the following item |
| `//! text` | Documentation for the enclosing module (top of file) |

In the MVP, documentation comments are kept by the lexer as doc tokens but are otherwise
treated as comments. A documentation tool (`sag doc`) is post-MVP.

## Keywords (F1/F2)

```
abstract  as        break     class     const     continue  else      enum
extends   extern    false     fn        for       if        impl      in
interface loop      match     new       override  pkg       prv       pub
return    self      Self      struct    true      unsafe    use       var
weak      where     while     zod
```

`zod` refers to the parent class (the role `super` has in other languages). `Self` refers to
the type being declared (see [types](02-types.md#self)).

## Reserved for future phases (post-MVP)

These must be rejected as identifiers so they can be introduced later without breaking code:

- Keywords: `async`, `await`.
- Standard names reserved for the concurrency model: `Send`, `Sync`, `spawn`.

## Identifiers and naming conventions

- Identifiers: a letter or `_` followed by letters, digits or `_`.
- Conventions (enforced as warnings, not errors): `snake_case` for functions, methods,
  variables and fields; `PascalCase` for types, enums, variants and interfaces;
  `SCREAMING_SNAKE_CASE` for module-level constants.

## Attributes (F1)

```sag
@export("sag_add")
extern "C" fn add(a: i32, b: i32) -> i32 { a + b }

@repr(C)
struct Timespec { pub tv_sec: i64; pub tv_nsec: i64; }
```

- Written `@name` or `@name(args)` immediately before the item they apply to (function,
  struct, enum, class, field, method).
- The set of attributes is **closed**: unknown attributes are a compile error. There are no
  user-defined attributes.
- MVP attributes: `@export("symbol")` and `@repr(C)` (see [FFI](10-ffi.md)).
- Reserved for later: `@test`, `@deprecated`.

## Numeric literals (F1)

| Element | Form |
|---|---|
| Bases | `255`, `0xFF`, `0o377`, `0b1111_1111` |
| Separator | `_` between digits: `1_000_000`, `0xFF_FF` |
| Type suffix | `255u8`, `10i64`, `1.5f32` |
| Floats | `3.14`, `1e10`, `2.5e-3`; a digit is required before the point (`0.5`, not `.5`) |

- Without a suffix or context, integer literals are `i32` and float literals are `f64`; with
  context they adopt the expected type (`const a: u8 = 5;`).
- A literal that does not fit its type (e.g. `256u8`) is a compile error, never a wrap.

## Other literals (F1)

- Booleans: `true`, `false`.
- Characters: `'a'` (a Unicode scalar value).
- Strings: `"plain"` is a literal string with no interpolation.
- Template strings: `` `total = ${expr}` `` interpolates expressions and produces a new `String`.
- Escapes, valid in `'...'`, `"..."` and `` `...` ``: `\n` `\r` `\t` `\\` `\0` `\"` `\'`
  `` \` `` `\$`, `\x7F` (ASCII only), `\u{1F600}` (Unicode). `\$` writes a literal `$` in a
  template string.
- No raw strings (`r"..."`) in the MVP.
