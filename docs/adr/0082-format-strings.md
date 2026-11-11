# 0082. Format strings `f"..."`

- Status: Accepted
- Date: 2026-11-10
- Refines: [ADR-0017](0017-resolution-of-open-questions-q1-q11.md) and docs/spec/01-lexical.md
  (template strings, `` `...${expr}...` ``, the only interpolated literal)

## Context

Strings are built with `+` (`"hello " + name`) or with template strings, written with backticks
as in JavaScript: `` `hello ${name}` ``. Rust, the reference of the language, writes the same
with braces inside a double-quoted format string: `format!("hello {name}")`, `println!("{x}")`.
Python (`f"hello {name}"`) and C# (`$"hello {name}"`) mark the literal with a prefix instead of
using a macro.

Plain literals cannot start interpolating braces: the compiler and the tests hold several
hundred `"..."` literals with braces (generated C, JSON), which would change meaning, and the seed
compiles the compiler's sources with today's rules, so the bootstrap would no longer reach its
fixed point.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q519 | Syntax | `f"..."` is a format string: `f` immediately followed by `"`, as Rust's `b"..."` prefixes. `f "..."`, with a space, stays the name `f` followed by a string. Plain `"..."` literals do not change. |
| Q520 | Interpolation | `{expr}` interpolates any expression, as `${expr}` does in templates: `f"{user.name} has {items.len()} items, {a + b} in total"`. The expression may contain braces and strings (`f"{if ok { "yes" } else { "no" }}"`). The types that can be interpolated and their format are those of templates. An empty `{}` is an error. |
| Q521 | Braces | `{{` and `}}` write `{` and `}`. A `}` that closes nothing is an error (`E0007`), as in Rust. |
| Q522 | Escapes | Those of `"..."`: `\n`, `\t`, `\"`, `\\`, `\u{...}` and the rest; `\u{...}` is an escape, not an interpolation. |
| Q523 | Format specifiers | None yet: no `{x:.2}` or `{x:?}`; a `:` inside the braces is part of the expression (`{a::b}`). Formatting options can come with a `Display` interface. |
| Q524 | Templates | Backtick templates keep compiling, with the same meaning, but are deprecated: the specification documents `f"..."` as the way to interpolate, and templates as the older form. The compiler does not warn about them, since the compiler, Nova and the harness use them and must build without warnings; migrating them is a separate step. |
| Q525 | Documents | This record, `docs/spec/01-lexical.md`, `docs/spec/02-types.md`, and the VS Code grammar. |

## Alternatives considered

- **Every `"..."` interpolates:** the shortest to write, but it changes the meaning of every
  literal with braces and breaks the bootstrap from the seed.
- **`format("...")` and format strings in `println`, as Rust's macros:** close to Rust, but
  `println("{")` would change meaning, and Sagitario has no macros; a function that parses its
  argument at compile time would be one.
- **Only names inside the braces (`{name}`, as Rust):** simpler, but templates already take any
  expression, and the two forms should agree.

## Consequences

- `f"hello {name}"` reads as Rust's format strings, without macros.
- The lexer turns a format string into the same token as a template, so the parser, the type
  checker and the code generation do not change.
