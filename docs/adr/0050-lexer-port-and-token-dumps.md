# 0050. The lexer port, token dumps and `char::from_u32`

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (Q216),
  [ADR-0013](0013-primitive-semantics.md) (`char::from_u32`)

## Context

F5 increment 2 starts the port: `span`, `diagnostics` and `lexer` in Sagitario, in the package
`compiler/`, checked against the Rust compiler token by token. Reading the Rust lexer showed two
gaps. Its identifiers and whitespace follow Unicode (`ñandú` is an identifier), while Sagitario's
`char` only has ASCII methods. And escapes such as `\u{1F600}` and UTF-8 decoding need a code
point turned into a `char`, which Sagitario could not do: `as char` only takes `u8`. Increment 2
settles Q226–Q233.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q226 | The dump | A hidden option, `sag check --dump=tokens file.sag`, lexes that file alone. It prints the tokens to stdout and the lexical diagnostics to stderr (in the chosen `--diagnostic-format`), with the exit codes of `check`. `sagc check --dump=tokens` does the same. Later passes add `ast`, `resolve` and `types`. |
| Q227 | Token format | One line per token: its byte range, kind and payload, such as `0..2 keyword fn`, `12..17 int 42 i64` or `20..26 str "a\nb"`. Text is quoted, with printable ASCII as is, `\"`, `\\`, `\n`, `\r`, `\t`, and `\u{hex}` otherwise. A template is a `template` line, followed by its parts (`text "..."`, `expr`) and the tokens of each `${}`, indented two spaces per level. |
| Q228 | `compiler/` | A Nova package, `sagc`, with one module per crate: `span.sag`, `diagnostics.sag` with `diagnostics/codes.sag`, `lexer.sag`, and `main.sag` (the command line). |
| Q229 | Text | The lexer reads the source as bytes (`List<u8>`) and decodes UTF-8 itself, so its offsets are the Rust spans. |
| Q230 | Diagnostics | Ported whole: diagnostics, their order, and both renderings, the human one byte for byte. Without a way to tell whether stderr is a terminal, `sagc` colors only when `CLICOLOR_FORCE` asks. |
| Q231 | Differential test | `crates/sagitario_cli/tests/selfhost.rs` builds `compiler/` with `sag`. Then, for every `.sag` file under `tests/`, `crates/sagitario_driver/src` and `compiler/`, it compares the exit code, stdout and stderr of both compilers, in both formats. `tests/lexer/` holds edge cases of the lexer. Lines end with `\n` in the comparison, since the C runtime writes `\r\n` on Windows. |
| Q232 | Rust changes | Only the dump, plus the fix below; where the Rust lexer has a quirk, `sagc` reproduces it. Two are known: `\x+7` is a valid escape, because Rust's `from_str_radix` accepts a leading `+`; and in `${...}` a `{` is counted again after whitespace, so braces inside an interpolation do not close as expected. |
| Q233 | `char::from_u32` | `char::from_u32(code: u32) -> Option<char>`, as in Rust: `None` for surrogates and values above `0x10FFFF`. It is a built-in function, like `List::from`. |
| Q234 | Unicode | `compiler/src/unicode.sag` and `compiler/src/diagnostics/codes.sag` are generated from Rust's `char` tables and from `sagitario_diagnostics::codes` by the test `selfhost`, which fails when they are out of date. `SAG_BLESS=1 cargo test --test selfhost` rewrites them. |
| Q235 | Deep nesting | The compiler runs on a thread with a 64 MiB stack. The passes recurse over the syntax tree, and the generated `unicode.sag` (conditions nested about ten levels, with long `\|\|` chains) overflowed the 1 MiB main thread of a debug build on Windows. |

## Alternatives considered

- **Unicode identifiers removed from the language:** a breaking change to work around a missing
  table.
- **`n.to_char()` or `n as char`:** no precedent in Rust, and `as` would have to choose between
  panicking and U+FFFD for invalid values.
- **Line and column in the dump:** the spans are byte offsets; lines and columns are compared
  through the diagnostics anyway.
- **Recursion turned into loops in every pass:** the passes stay simple and portable; a large
  stack is what rustc does too.

## Consequences

- `sagc` represents an integer literal's value as decimal text, since Sagitario has no `u128`,
  and keywords and punctuation as text.
- `sagc` needs no generated file at run time; the generated modules are ordinary source.
- If CI's Rust has a newer Unicode version than the one that generated `unicode.sag`, the test
  fails there until the file is regenerated.
- The test builds `compiler/src/main` (ignored by git) next to the sources.
