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

## Keywords (F1/F2, F8)

```
abstract  as        async     await     break     class     const     continue
else      enum      extends   extern    false     fn        for       if
impl      in        interface let       loop      match     new       override
pkg       prv       pub       return    self      Self      static    struct
super     true      unsafe    use       var       weak      where     while
zod
```

`async` and `await` are those of [`async` functions](12-async.md) (F8), and `static` declares a
[static](03-memory.md#global-state-g1) (G1, [ADR-0083](../adr/0083-global-state.md)).

`union` is a contextual keyword, as in Rust: it declares a [union](10-ffi.md#unions) only before
the name of one, and is an ordinary name elsewhere.

`include`, `include_str`, `include_bytes`, `concat` and `env` are the names of the
[built-in macros](09-modules.md#built-in-macros-s4) when `!(` follows them (`include_str!("file")`),
and ordinary names elsewhere. `...` ends the parameters of a
[variadic C function](10-ffi.md#variadic-functions).

`let` only appears in `if let` and `while let` (F5,
[control flow](06-control-flow.md#if-let-and-while-let)); bindings are `const` and `var`.

`zod` refers to the parent class (the role `super` has in other languages); `super` refers to
the parent module (see [modules](09-modules.md#paths)), and `pkg` to the package root. `Self`
refers to the type being declared (see [types](02-types.md#self)).

## Reserved for future phases (post-MVP)

These must be rejected as identifiers so they can be introduced later without breaking code:

- No keywords at the moment: `async` and `await`, reserved until F8, are keywords of
  [`async` functions](12-async.md) ([ADR-0072](../adr/0072-async-await-increments.md)).
- Names of the concurrency model, which programs cannot declare: `Send` (the prelude's
  interface, F4) and `Sync` (reserved). `spawn`, reserved until F4, is now
  `std::thread::spawn` and an ordinary name ([ADR-0045](../adr/0045-threads-and-send.md)).

## Identifiers and naming conventions

- Identifiers: a letter or `_` followed by letters, digits or `_`.
- Conventions (enforced as warnings, not errors): `snake_case` for functions, methods,
  variables and fields; `PascalCase` for types, enums, variants and interfaces;
  `SCREAMING_SNAKE_CASE` for module-level constants.

## Attributes (F1)

```sag
@export("demo_add")
extern "C" fn add(a: i32, b: i32) -> i32 { a + b }

@repr(C)
struct Timespec { pub tv_sec: i64; pub tv_nsec: i64; }
```

- Written `@name` or `@name(args)` immediately before the item they apply to (function,
  struct, enum, class, field, method).
- The set of attributes is **closed**: unknown attributes are a compile error. There are no
  user-defined attributes.
- Attributes: `@cfg(condition)` on items ([conditional compilation](09-modules.md#conditional-compilation-s1));
  `@default` on structs, enums and the variants of enums ([defaults](05-interfaces-generics.md#defaults));
  `@export("symbol")`; `@repr(C)` on structs and unions, and `@repr(C)` or an
  integer type (`@repr(i32)`) on enums (see [FFI](10-ffi.md)); `@thread_local` on a
  `static var` (see [global state](03-memory.md#global-state-g1)).
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
- Strings: `"plain"` is a literal string with no interpolation; its braces are text.
- Format strings ([ADR-0082](../adr/0082-format-strings.md)): `f"total = {expr}"` interpolates
  expressions and produces a new `String`. The prefix is `f` immediately followed by `"` (`f "x"`
  is the name `f` and a string). Each `{...}` holds any expression, which may contain braces and
  strings; `{{` and `}}` write `{` and `}`, and a `}` that closes nothing or an empty `{}` is an
  error (`E0007`). A format specifier may follow the expression after a `:`, `{x:>8.2}`
  ([ADR-0089](../adr/0089-display-debug-and-format-specifiers.md)): the first `:` outside
  parentheses, brackets, braces and strings that is not part of `::`, so `{a::b}` names a path.
  Template strings have no specifiers.
- Template strings: `` `total = ${expr}` `` interpolates expressions too. They are
  **deprecated**: they keep compiling with the same meaning, but new code writes format strings.
- Escapes, valid in `'...'`, `"..."`, `f"..."` and `` `...` ``: `\n` `\r` `\t` `\\` `\0` `\"`
  `\'` `` \` `` `\$`, `\x7F` (ASCII only), `\u{1F600}` (Unicode). `\$` writes a literal `$` in a
  template string.
- No raw strings (`r"..."`) in the MVP.
