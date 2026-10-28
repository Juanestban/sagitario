# Parser corpus

Edge cases of the parser: syntax errors and recovery (`recovery.sag`, with every `E01xx` code),
choices the grammar makes (`expressions.sag`) and interpolations (`templates.sag`)
(docs/adr/0051-parser-port-and-ast-dumps.md). The harness in Sagitario compares the `--dump=ast` of
each case, with its diagnostics, with `<case>.expected` next to it
(docs/adr/0066-test-harness-in-sagitario.md).
