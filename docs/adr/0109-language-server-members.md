# 0109. What the language server knows of members

- Status: Accepted
- Date: 2026-11-21
- Refines: [ADR-0058](0058-language-server.md) (completion from the text around the cursor, types
  from the last analysis that reached the type checker)

## Context

The language server answered from names: after `.` it took the binding before the dot and walked
the fields and methods after it by their names, and the builtin methods were lists of names. Using
the standard library, the bindings of `bindgen` and chains of calls, users saw:

- no members after a chain of builtin methods (`words.first().unwrap().`) or after a call of a
  path (`fs::read_to_string(path).`), and `{error}` or a type parameter (`T`) as the type of a
  generic method (`holder.get()`), so no `unwrap` after `Option` or `Result` values;
- builtin methods without their signature or documentation, `into` and `try_into` first;
- hover with the type of an expression only, nothing on C functions, `CString::new` or variants,
  and no documentation anywhere;
- nothing at all while the document had a syntax error, which is most of the time while typing:
  the analysis stopped before resolving.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q775 | Receivers | The type of the receiver before `.` is the type the checker gave the expression ending at the dot, in an analysis that read the text up to there. When the latest analysis did not, the server analyzes the text without the member being typed (`receiver.name` becomes `receiver`, with a `;` if the statement lacks one, and without the `)` the editor closed), and uses that. The names before the dot remain the fallback. |
| Q776 | Members | Every member shown has its signature with the receiver's type arguments substituted (`fn unwrap(self) -> String` on an `Option<String>`), and its documentation: the `///` comments of the declaration for the program's and the standard library's, and a table of the compiler's for builtin methods, static functions (`CString::new`, `i32::MAX`, `String::from_utf8`) and builtins (`println`). The methods of a value come first, and the conversions every value has (`into`, `try_into`) after them. |
| Q777 | Syntax errors | The parser skips what it cannot read; with syntax errors, the analysis still resolves and checks the rest, into diagnostics nobody sees, since only the syntax errors are reported, as in a build. The server keeps the latest analysis without syntax errors for the questions about the rest of the document. |
| Q778 | Hover | Hover shows the declaration (the signature of a function, method or C function, a field with its type, a variant as written with its enum) in a Sagitario code block, then its documentation. References to the project's records, `(docs/...)` and `(Q123)`, are left out of documentation. |
| Q779 | Signatures | `textDocument/signatureHelp`, after `(` and `,`: the signature of the function, C function, builtin or method being called, its documentation and the parameter of the argument at the cursor. |

## Alternatives considered

- **Error-tolerant parsing that keeps the partial statement** (`receiver.` as a field without a
  name): it would change what the parser gives the compiler and its recovery dumps, for one tool.
  Analyzing a patched copy of the text reaches the same type without touching the compiler.
- **Generating the table of builtin methods from the type checker**: the checker knows their
  signatures but not their documentation, and its methods are matched by name in code, not kept as
  data. The table states the documentation once, next to the signature it shows.

## Consequences

- Completion after `.` may analyze the program once more, synchronously; it costs what an
  analysis after a change costs.
- A builtin method added to the type checker must also be added to the table of
  `compiler/src/lsp/builtins.sag`, as the editor rules of CLAUDE.md already ask of every API.
- A document opened with a syntax error that cuts a function short has no analysis of that
  function until it parses once; the answers then come from that analysis.
