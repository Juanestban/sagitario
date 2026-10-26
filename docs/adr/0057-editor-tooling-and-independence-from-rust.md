# 0057. Editor tooling and independence from Rust (phase F6)

- Status: Accepted
- Date: 2026-10-26
- Supersedes: Q208 of [ADR-0048](0048-async-await.md) (`async`/`await` moves from F6 to F7)

## Context

F5 ended with a compiler written in Sagitario that reaches its fixed point (ADR-0056). Two needs
come before the next language phase: writing Sagitario comfortably, with an editor that knows
the language, and stopping depending on Rust, so that the language is developed in itself and
each change goes into one compiler instead of two. Q293–Q303 plan the phase and settle its first
increment, the VS Code extension.

## Decision

### The phase

| # | Topic | Decision |
|---|---|---|
| Q293 | Order | A new phase F6 in three parts: (1) the VS Code extension, (2) a language server written in Sagitario, (3) the pivot without Rust. `async`/`await` (ADR-0048) moves to F7. |
| Q300 | Language server (increment 2) | `sag lsp`, written in Sagitario on the front end of `sagc`: diagnostics while typing, the type under the cursor, go to definition, completion (keywords, bindings, members after `.`, paths after `::`) and the symbols of a file, over JSON-RPC on stdin and stdout, with the JSON written inside the server. Its own decision record details it. |
| Q301 | What the server needs from `std` | LSP messages have a length and no final newline, and `std::io` reads only lines or everything. Increment 2 adds a way to read a number of bytes (such as `io::read_exact(n)`), with its record and its specification. |
| Q302 | The VS Code client | From increment 2, a small TypeScript client in the extension starts the server through `vscode-languageclient`, Microsoft's library, approved as a dependency. VS Code loads only JavaScript; everything else is Sagitario. |
| Q303 | The pivot (part 3) | Detailed in its own records: (a) the generated C of `sagc` committed as a seed, so a C compiler alone builds the compiler; (b) Nova ported to Sagitario; (c) the test harness ported and `cargo` out of CI; (d) `sagc` renamed to `sag` and `crates/` retired, by a record superseding Q214 of ADR-0049. Until then, language changes still go into both compilers. |

### Increment 1: the VS Code extension

| # | Topic | Decision |
|---|---|---|
| Q294 | Where | `editors/vscode/` in this repository, with its own `package.json`, next to the specification it follows and the CI that checks it. |
| Q295 | Contents | Declarative files only, without JavaScript or runtime dependencies: the language `sagitario` for `.sag` files, the file icon, a TextMate grammar, the language configuration and snippets (`main`, `mainr`, `fn`, `pfn`, `struct`, `class`, `enum`, `interface`, `match`, `iflet`, `whilelet`, `for`, `forin`, `if`, `ifelse`, `println`, `use`). |
| Q296 | Grammar | Written from `docs/spec/01-lexical.md`: keywords, primitive types, numeric literals with their suffixes, characters, strings and template strings whose `${...}` are colored as code, nested block comments, documentation comments, attributes, and the names of declared types and functions, calls, paths and constants. The test `vscode_grammar_knows_every_keyword` (`crates/sagitario_cli/tests/editor.rs`) checks that every keyword of the specification is in the grammar. |
| Q297 | Language configuration | `//` and `/* */` for the comment commands; matching and auto-closing `()`, `[]`, `{}`, `""`, `''` and backquotes; folding by braces; the indentation of the repository (2 spaces); `///` and `//!` continue on Enter. |
| Q298 | Packaging | A `.vsix` built by `@vscode/vsce`, a development dependency approved for it and not shipped in the extension, and installed with `code --install-extension`. A CI job packages it. |
| Q299 | Marketplace | Not published yet: publishing needs a publisher account of the author's. The manifest is ready for it. |
| — | Icon | The file icon is a 16×16 drawing of its own: the bow and arrow of the sign of Sagittarius on the night sky of the logo (`docs/assets/logo.svg`), with the cursor of a prompt. The extension's own icon is the logo, as a 128×128 PNG. |

## Alternatives considered

- **The language server first, the extension later:** the extension without code is useful at
  once, and the server needs a change to `std`.
- **A language server in Rust, on the existing crates:** faster to write, but it would deepen the
  dependency the phase removes.
- **A hand-written LSP client instead of `vscode-languageclient`:** about 300 lines to maintain
  for what the official library already does.

## Consequences

- `async` and `await` stay reserved keywords until F7.
- The grammar is the source of highlighting; a keyword added to the language must be added to
  it, which the test enforces.
- `npm audit` reports a vulnerability in `@azure/msal-node`, which `vsce` uses only to publish
  with Azure credentials; packaging does not run it. It is revisited when the extension is
  published.
