# Sagitario for Visual Studio Code

Support for the [Sagitario](https://github.com/Juanestban/sagitario) programming language:

- The language server (`sag lsp`): the errors and warnings of `sag check` while typing, the type
  of what is under the cursor, go to definition (`F12` or `Ctrl+Click`, also on the modules of
  `use` and of paths), completion of members after `.`, of items after `::`, of names and of
  types, the outline of a document (`Ctrl+Shift+O`), and the colors of modules, functions and
  types in paths (semantic tokens).
- Syntax highlighting of `.sag` files: keywords, types, numbers with their suffixes, characters,
  strings and template strings with their `${...}` interpolations, nested block comments,
  documentation comments and attributes.
- An icon for `.sag` files in the explorer and the tabs.
- Commenting with `Ctrl+/`, matching and auto-closing brackets and quotes, and folding.
- Snippets: `main`, `mainr`, `fn`, `pfn`, `struct`, `class`, `enum`, `interface`, `match`,
  `iflet`, `whilelet`, `for`, `forin`, `if`, `ifelse`, `println` and `use`.

## The language server

The extension starts `sag lsp`. Build and install `sag` from the repository with a C compiler,
in release mode, which analyses large programs about three times faster than a debug build:

```sh
sh bootstrap/build.sh --install      # Linux and macOS: ~/.sagitario/bin, added to PATH
bootstrap/build.ps1 --install        # Windows (PowerShell): %USERPROFILE%\.sagitario\bin
```

Or set `sagitario.server.path` to a `sag` executable elsewhere.
`Sagitario: Restart the language server` restarts it. Without the server, highlighting,
snippets and the icon keep working (docs/adr/0058-language-server.md).

The file icon is shown by file icon themes that leave unknown files to the languages, such as
the default Seti theme. Themes with their own icon for every file keep theirs. Material Icon
Theme takes an icon of another extension through a path from its `dist` folder, which names the
installed version of this one:

```json
"material-icon-theme.files.associations": {
  "*.sag": "../../sagitario.sagitario-0.2.0/icons/sag-file"
}
```

Highlighting tells names apart by their form, as most grammars do:
`snake_case` names are bindings, parameters and fields, `PascalCase` names are types and
variants, `SCREAMING_SNAKE_CASE` names are constants, and a name before `(` is a function.

## Building and installing

From this directory:

```sh
npm ci
npm run package                           # writes sagitario.vsix
code --install-extension sagitario.vsix
```

## License

Licensed under either of MIT or Apache-2.0, at your option, as the rest of the repository
(`LICENSE-MIT` and `LICENSE-APACHE` at its root).
