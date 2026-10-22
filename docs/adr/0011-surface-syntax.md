# 0011. Surface syntax

- Status: Accepted
- Date: 2026-10-21

## Context

The syntax blends Rust (types, `fn`, `match`, expression blocks) with TypeScript/C# (classes,
lambdas, template strings), and should be simple to parse in both the Rust compiler and the
future self-hosted compiler.

## Decision

- `{}` blocks, mandatory `;`, the last expression without `;` is the block value.
- `fn`, `name: Type`, `-> T`; Rust primitive names and naming conventions.
- `self` everywhere (no `this`); `zod` for the parent class (`zod.new(...)` first in the
  constructor, `zod.method()`).
- Constructor `pub fn new(...)` as an initializer with implicit `self`; `new Class(...)`
  only for classes; structs use literals.
- No `impl` blocks: methods live inside `struct` / `class` bodies. `impl` names interfaces:
  `class A extends B impl I1, I2`.
- `pub` / `prv` mandatory on class members.
- Lambdas `(x) => expr`; template strings with backticks; plain strings with `"`.

## Alternatives considered

- **`this`, `super`, `constructor`, `implements`, `open`** (initial proposal): replaced
  according to the author's preferences (`self`, `zod`, `fn new`, `impl`, no `open`).
- **Optional semicolons**: require insertion rules that complicate the parser.
- **`new` for structs too (C#)**: rejected so that `new` always signals an ARC allocation.

## Consequences

- One-token lookahead-friendly grammar suited to a hand-written parser.
- `zod` is a unique keyword that newcomers must learn.
