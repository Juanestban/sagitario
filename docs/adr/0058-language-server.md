# 0058. The language server

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0057](0057-editor-tooling-and-independence-from-rust.md)

## Context

F6 increment 2 gives the editor what highlighting cannot: the errors of the program while it is
written, the type of what is under the cursor, where a name is declared, completion and the
outline of a document. ADR-0057 settled that the server is written in Sagitario and that the VS
Code extension starts it through `vscode-languageclient`. Two things stood in the way: LSP frames
messages by their length in bytes, which `std::io` could not read, and on Windows the C runtime
translated `\n` into `\r\n` on stdout, which would have broken every length. Increment 2 settles
Q304–Q317.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q304 | Form | A command of the compiler written in Sagitario, `sagc lsp`, that reuses its lexer, parser, resolver and type checker as they are: one executable, which becomes `sag` with the pivot. The Rust `sag` has no `lsp`. |
| Q305 | Features | Diagnostics while typing, the same as `sag check`; hover; go to definition, also across files; completion; the symbols of a document. Renaming, references, formatting and the documentation of `///` comments in hover come later; the parser does not keep those comments yet. |
| Q306 | Completion | Keywords; the names visible at the cursor (items of the module, imports, the prelude, and the bindings of the function declared before the cursor); members after `.` (fields, methods and built-in methods of the type); and what follows `::` (the public items of another module, the variants and functions of a type, the modules of `std`). The text before the cursor tells which, since code being typed rarely compiles; a receiver is a chain of bindings, fields and method calls (`counter.origin().`), typed with the last analysis of the document that reached the type checker. |
| Q307 | When it analyses | On open, on change and on save, with the whole text (`Full` synchronization), in one thread, the whole program at once with the open documents in place of their files. A program of a few files takes about 20 ms; the compiler itself (60 modules) about 1.4 s with a release `sagc`. Analysing in another thread is a later improvement. |
| Q308 | Positions | LSP counts columns in UTF-16 code units, the only encoding VS Code accepts. `lsp/positions.sag` converts between them and the byte offsets of the compiler's spans. |
| Q309 | JSON | `compiler/src/lsp/json.sag`, a parser and a writer of its own. Numbers keep their text, so ids and positions are written back exactly. |
| Q310 | `io::read_exact` | `io::read_exact(count: usize) -> Result<String, io::Error>`: exactly `count` bytes of stdin; an input that ends before is an error of kind `UnexpectedEof` (Rust's message, `failed to fill whole buffer`), bytes that are not UTF-8 one of kind `InvalidData`, and the bytes read are lost either way. |
| Q311 | Standard streams byte for byte | Every Sagitario program reads and writes stdin, stdout and stderr byte for byte on every system: on Windows the runtime puts them in binary mode when it starts (`_setmode`, declared as the runtime already declares other functions of the system). `\n` is written as `\n` everywhere, as Rust writes it, and `read_line` already accepts `\n` and `\r\n`. |
| Q312 | Robustness | The server does not end because of what is written in the editor: an internal compiler error of the lowering goes to `window/logMessage` and the server goes on, a malformed message gets a JSON-RPC error (`-32700`, `-32600`, `-32601`), and `shutdown` and `exit` are honored (exit code 0 after `shutdown`, 1 without it). |
| Q313 | VS Code client | `editors/vscode/extension.js`, plain JavaScript, so no TypeScript or type packages are added: only `vscode-languageclient`. It starts the command of the setting `sagitario.server.path` (`sagc` on PATH by default) with `lsp`; when it cannot, it says how to build `sagc`, and highlighting keeps working. A command restarts the server, and so does changing the setting. |
| Q314 | Installing `sagc` | Until the pivot, the README of the extension explains how to build `sagc` (`sag build --release compiler/src/main.sag`) and point the setting at it. |
| Q315 | Tests | `tests/lsp/`: each case is a `.sag` document, a `.session` of LSP messages (one per line, with `{uri}` and `{text:file}` placeholders and `raw` bodies sent as written) and the `.expected` messages of the server, one per line. The test `language_server_answers_the_sessions` (`crates/sagitario_cli/tests/selfhost.rs`) runs them; `SAG_BLESS=1` rewrites the expected files. They cover diagnostics, hover, definitions, completion in code that compiles and in code being typed, symbols and malformed messages. |
| Q316 | Files | `compiler/src/lsp.sag` (the server and its dispatch) and `compiler/src/lsp/{json, protocol, positions, analysis, query, completion}.sag`; in the extension, `extension.js` and its settings in `package.json`. |
| Q317 | Documentation | This record; the specification of `std::io` (Q310, Q311); `docs/architecture.md`; `docs/testing.md`; the README of the extension; the roadmap. |

## Alternatives considered

- **A separate package for the server:** it would duplicate the compiler's modules, which are
  not a library.
- **`std::io` writing raw bytes only for the server:** every program would still write `\r\n` on
  Windows and `\n` elsewhere.
- **Incremental synchronization:** it saves sending the text, not analysing it, which is what
  takes the time.

## Consequences

- The program a document belongs to is the package whose `Nova.toml` is in one of its
  directories (`src/main.sag`, or `src/lib.sag` as a library), else the `main.sag` next to it, else
  the file alone; a file that program does not reach is analysed alone.
- The loader of `sagc` takes texts that replace files on disk (`Loader::overrides`), and
  `TargetInfo` stays the host's.
- On Windows the output of every Sagitario program changes from `\r\n` to `\n`.
- Analysing the whole compiler after every keystroke takes over a second, so typing in its files
  waits for the analyses; a thread for them is the next improvement of the server.
