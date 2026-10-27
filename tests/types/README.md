# Type checker corpus

Edge cases of the type checker. Its declarations (F5 increment 5a): constants at the limits of every
integer type, float constants and casts (`constants.sag`), classes, inheritance and vtables
(`classes.sag`), interfaces, generics and their bounds (`interfaces.sag`), and the errors of
declarations (`declaration_errors.sag`). The bodies of functions (increment 5b): inference,
generics, closures, collections and the errors of expressions (`bodies.sag`), and the flow analysis,
exhaustiveness, methods that modify `self` and threads (`flow.sag`). They are not a test suite of
their own: the test `selfhost` (`crates/sagitario_cli/tests/selfhost.rs`) checks that `sagc` types
them, and every other `.sag` file of the repository, exactly as `sag` does
(docs/adr/0053-type-checker-port-and-type-dumps.md). The harness in Sagitario compares the
`--dump=types` of each case, with its diagnostics, with `<case>.expected` next to it
(docs/adr/0066-test-harness-in-sagitario.md).
