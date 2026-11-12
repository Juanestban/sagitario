# Sagitario for Visual Studio Code

Support for the [Sagitario](https://github.com/Juanestban/sagitario) programming language:

- The language server (`sag lsp`): the errors and warnings of `sag check` while typing, the type
  of what is under the cursor, go to definition (`F12` or `Ctrl+Click`, also on the modules of
  `use` and of paths), completion of members after `.` (the methods of numbers, text, characters, lists, arrays, slices, maps, iterators, `Option` and
  `Result` too, `into` and `try_into` on every value, and the
  indices of a tuple's elements), of items after `::` (`MIN` and `MAX` after an integer type, the functions of `std::mem` and `std::hint`, the static functions of a type's interfaces and of a type parameter's bounds after `Type::` and `T::`, and `from` and `try_from` after the types of numbers, `char` and `String`), of attributes after
  `@` (`@cfg`, `@export`, `@repr`, `@thread_local`), of their representations after `@repr(` and
  of the conditions of `@cfg(`, of names
  (module-level constants and statics with their types, and the builtins, `assert` and `assert_eq` among them) and of types, the outline of a document (`Ctrl+Shift+O`), and the colors of modules, functions and
  types in paths (semantic tokens). Inside the braces of format strings, as rust-analyzer does
  in Rust's, it also colors the variables, parameters, fields, methods and functions.
- Syntax highlighting of `.sag` files: keywords, types, operators (the bitwise ones and shifts
  too), numbers with their suffixes, the indices of tuples' elements (`pair.0`) and the names a tuple pattern declares
  (`const (q, r) = ...`), characters,
  strings, format strings `f"...{expr}..."` with their specifiers (`{x:>8.2}`) and template strings with their interpolations,
  whose expressions are highlighted as code and whose braces as punctuation, nested block
  comments, documentation comments and attributes.
- An icon for `.sag` files in the explorer and the tabs.
- Commenting with `Ctrl+/`, matching and auto-closing brackets and quotes, and folding.
- Snippets: `main`, `mainr`, `asyncmain`, `asyncfn`, `fn`, `pfn`, `static`, `staticvar`,
  `threadlocal`, `cfg`, `struct`, `class`, `enum`, `interface`, `implfor`, `match`, `iflet`, `whilelet`, `for`,
  `forin`, `fortuple`, `consttuple`, `assert`, `asserteq`, `if`, `ifelse`, `println`, `fstr` (a format string) and `use`; for operators and text, `impleq`, `implord`, `impldisplay`, `implop` (`+`, `-`, `*`,
  `/`, `%`) and `implneg`; for conversions, `implfrom` and `impltryfrom`; and for the FFI, `extern` (a block of C functions), `externstatic` (a global variable of C), `externfn`,
  `export`, `reprc`, `union`, `reprenum`, `cfn`, `unsafe`, `cstring` and `library` (a function
  loaded from a dynamic library).

## The language server

The extension starts `sag lsp`, with the `sag` of the same version of Sagitario. Install it with
one command:

```sh
curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh | sh   # Linux and macOS
```

```powershell
irm https://github.com/Juanestban/sagitario/releases/latest/download/install.ps1 | iex         # Windows
```

The extension finds it in `~/.sagitario/bin` (`%USERPROFILE%\.sagitario\bin` on Windows) without
settings, also before the editor sees the new `PATH`; `bootstrap/build.sh --install` and
`build.ps1 --install` put a build from source there too. Otherwise it runs `sag` on `PATH`, or the
executable that `sagitario.server.path` names. After `nova update self`,
`Sagitario: Restart the language server` starts the new `sag`. Without the server, highlighting,
snippets and the icon keep working (docs/adr/0058-language-server.md).

The file icon is shown by file icon themes that leave unknown files to the languages, such as
the default Seti theme. Themes with their own icon for every file keep theirs. Material Icon
Theme takes an icon of another extension through a path from its `dist` folder, which names the
installed version of this one (here 0.9.0):

```json
"material-icon-theme.files.associations": {
  "*.sag": "../../sagitario.sagitario-0.9.0/icons/sag-file"
}
```

Highlighting tells names apart by their form, as most grammars do:
`snake_case` names are bindings, parameters and fields, `PascalCase` names are types and
variants, `SCREAMING_SNAKE_CASE` names are constants, and a name before `(` is a function.

## Building and installing

Each [release of Sagitario](https://github.com/Juanestban/sagitario/releases) has its
`sagitario.vsix`, with its version: install it with `code --install-extension sagitario.vsix`. To
build it from this directory:

```sh
npm ci
npm run package                           # writes sagitario.vsix
code --install-extension sagitario.vsix
```

From the root of the repository, one command does the three steps (`npm ci` only when
`node_modules` is missing) and replaces the installed extension; then reload the windows of VS
Code:

```sh
sag run tools/vscode.sag                          # --no-install only packages it
sag run tools/vscode.sag -- --code code-insiders  # another VS Code
```

## License

Licensed under either of MIT or Apache-2.0, at your option, as the rest of the repository
(`LICENSE-MIT` and `LICENSE-APACHE` at its root).
