# 1. Lexical structure

## Source files

- Source files use the `.sag` extension and must be valid UTF-8.
- Statements end with `;`. There is no automatic semicolon insertion.
- Blocks are delimited by `{` and `}`.

## Keywords (F1/F2)

```
abstract  as        break     class     const     continue  else      enum
extends   extern    false     fn        for       if        impl      in
interface loop      match     new       override  pkg       prv       pub
return    self      struct    true      unsafe    use       var       weak
where     while     zod
```

`zod` refers to the parent class (the role `super` has in other languages).

## Reserved for future phases (post-MVP)

These must be rejected as identifiers so they can be introduced later without breaking code:

- Keywords: `async`, `await`.
- Standard names reserved for the concurrency model: `Send`, `Sync`, `spawn`.

## Identifiers and naming conventions

- Identifiers: a letter or `_` followed by letters, digits or `_`.
- Conventions (enforced as warnings, not errors): `snake_case` for functions, methods,
  variables and fields; `PascalCase` for types, enums, variants and interfaces;
  `SCREAMING_SNAKE_CASE` for module-level constants.

## Literals

- Integers: `42`. Floats: `3.14`. Without context, integer literals are `i32` and float
  literals are `f64`; with context they adopt the expected type (`const a: u8 = 5;`).
- Booleans: `true`, `false`.
- Characters: `'a'` (a Unicode scalar value).
- Strings: `"plain"` is a literal string with no interpolation.
- Template strings: `` `total = ${expr}` `` interpolates expressions and produces a new `String`.

Escape sequences, numeric prefixes and separators: see open question Q6 in the
[index](README.md#open-questions). Comments: see Q1.
