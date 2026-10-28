# Lexer corpus

Edge cases of the lexer, valid and invalid (docs/adr/0050-lexer-port-and-token-dumps.md). The
harness in Sagitario compares the `--dump=tokens` of each case, with its diagnostics, with
`<case>.expected` next to it (docs/adr/0066-test-harness-in-sagitario.md). The files keep `\n` line endings on every system, and
`crlf.sag` keeps `\r\n` (`.gitattributes`).
