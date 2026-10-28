# 0059. Paths and semantic tokens in the language server

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0058](0058-language-server.md)

## Context

Using the extension of ADR-0058 on the compiler's own code showed what it lacked against Rust:
the names of `use pkg::loader::push_path` and `std::fs` took the color of a variable, since a
grammar cannot tell a module from a function by its form; `Ctrl+Click` on a module of a `use` or
of a path did not open it; and completion offered no types of the language. Q318–Q321 settle
them.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q318 | Semantic tokens | The server provides `textDocument/semanticTokens/full`, as rust-analyzer does, for what the grammar cannot know: each segment of a `use` and the segments of paths with several (`diagnostics::render`, `Shape::Circle`), typed by what they resolve to: `namespace`, `function`, `struct`, `class`, `enum`, `interface`, `type` (C types) and `enumMember`. Single names keep the colors of the grammar. Tokens come only from an analysis that read the current text; otherwise the answer is `ContentModified` (`-32801`) and the editor asks again. |
| Q319 | Paths in hover and definitions | Hover and go to definition work on every segment of a `use` and on the segments of a path before its last: a module shows `module pkg::loader` and goes to the start of its file, an imported function, type or constant to its declaration. The modules of `std` are embedded in the compiler and have no file to go to. |
| Q320 | Types | Completion offers the types of the language (`i32`, `String`, `List`, `Option`...), and hover on one of them in a type, or on a type parameter, names it. |
| Q321 | Grammar | A `use` declaration has a rule of its own: its names are modules by their form (or types and constants by theirs), never variables, for editors without the server. The prelude's `Some`, `None`, `Ok` and `Err` are `constant.other.symbol`, which themes color as Rust's enum members. |

## Alternatives considered

- **Semantic tokens for every name:** locals, parameters and fields already have the colors of
  their form, and tokens for them would add work to every analysis without changing what is
  shown.

## Consequences

- `compiler/src/lsp/tokens.sag` collects the tokens; the resolution of paths of a module
  (`query::resolve_path`) serves tokens, hover, definitions and completion after `::`.
- `tests/lsp/paths.sag` and its module `paths_util.sag` cover them; expected answers write the
  directory of a case as `{dir}`.
