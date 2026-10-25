# 0051. The parser port and AST dumps

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0050](0050-lexer-port-and-token-dumps.md)

## Context

F5 increment 3 ports the parser (about 2 700 lines of Rust) and the syntax tree (46 types). The
parser recovers from errors at item, member and statement boundaries, and parses speculatively
in one place: `<` after a name in an expression is tried as type arguments, and on failure the
position and diagnostics are restored. Node ids are not restored, so they must be allocated in
exactly the same order by both parsers. Increment 3 settles Q236–Q242.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q236 | The dump | `sag check --dump=ast file.sag` parses that file alone, without loading modules. It prints the tree to stdout and the lexical and syntax diagnostics to stderr. As in a build, a file with lexical errors is not parsed. |
| Q237 | Format | An indented tree, one node per line: `<field>: <Node> <start>..<end> [#<id>] [values]`. A list repeats its field, an absent option is left out, names are `name@start..end`, text is quoted as in the token dump, and variants by their name or symbol (`op=+`, `pub`). A hand-written dumper in `sagitario_ast::dump` writes it, not `{:#?}`: Rust's `Debug` escapes text by Unicode printability, which would be another table to port. |
| Q238 | Node ids | They are in the dump. Later passes key their results by node id, so both compilers must number nodes identically, including the ids spent by an abandoned speculative parse. |
| Q239 | The tree in Sagitario | `Expr`, `Pattern` and `TypeExpr` are classes instead of `Box`: a struct or enum cannot contain itself by value, and a class is a reference. `Option<Box<T>>` becomes `Option<T>`. The other nodes are structs and enums with named fields, as in Rust. |
| Q240 | Files | `compiler/src/ast.sag`, `compiler/src/ast/dump.sag` and `compiler/src/parser.sag`, the parser in one file: the methods of a Sagitario class are all in its body. |
| Q241 | Corpus | `tests/parser/`: syntax errors and recovery with every `E01xx` code, choices of the grammar (generics against comparisons, struct literals in conditions, lambdas against parentheses), and errors inside interpolations. With every `.sag` file of the repository, in both diagnostic formats. |
| Q242 | Rust changes | Only the dump. `sagc` reproduces the parser as it is. |

## Alternatives considered

- **Comparing the trees without ids:** a difference in numbering would only show in later
  passes, far from its cause.
- **A generic dump through a reflection-like trait:** Sagitario has no such mechanism, and the
  two dumpers would not be written the same way.
- **Splitting the parser into several files with free functions taking the parser:** further
  from the Rust code it is checked against.

## Consequences

- In `sagc`, a parse error is `Err(())`: the error is already reported. Attributes return what
  they set instead of writing through mutable references, and a function's body is chosen with
  an enum instead of a closure.
- Two AST fields are renamed in Sagitario, since `weak` and `var` are keywords there:
  `FieldDecl::is_weak` and `ExprKind::For::variable`.
- `sagc`'s `Module` holds only what the parser fills; the loader adds the module's path, package
  and location when the driver is ported.
