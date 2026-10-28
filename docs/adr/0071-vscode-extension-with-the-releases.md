# 0071. The VS Code extension with the releases

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0068](0068-releases-and-distribution.md) (increment 4 of phase F7, Q379)

## Context

Releases ([ADR-0068](0068-releases-and-distribution.md)), the installers
([ADR-0069](0069-installers.md)) and `nova update self` ([ADR-0070](0070-nova-update-self.md)) put
`sag` and `nova` in `~/.sagitario/bin`. The VS Code extension in `editors/vscode/`
([ADR-0058](0058-language-server.md)) is installed from a `.vsix` built by hand, carries a version
of its own (0.2.0), and starts `sag lsp` only through `PATH` or the setting
`sagitario.server.path`. `PATH` is not enough: the installers add `~/.sagitario/bin` to the shell
profiles and to the `PATH` of the user, which an editor already running does not see, and which
macOS applications started from the Dock never read. Increment 4 planned to publish the extension
in the Marketplace; that is not planned for now.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q410 | The Marketplace | Not decided. The extension is not published in the Visual Studio Marketplace or Open VSX; a later record may decide it. The acceptance of F7 does not require it. |
| Q411 | Version | The version of Sagitario (Q380): `editors/vscode/package.json` and its `package-lock.json` say the same as `compiler/Nova.toml`. The test `cli::the_vscode_extension_has_the_version` of the harness checks it. |
| Q412 | The `.vsix` of a release | A job `vscode` of the release workflow packages the extension (`npm ci`, `npm run package`, one package for every platform, since it is plain JavaScript), and the job that publishes the release attaches `sagitario.vsix` to it. It is installed with `code --install-extension sagitario.vsix`. |
| Q413 | Finding `sag` | The setting `sagitario.server.path`, when it is set (its default becomes empty); else `~/.sagitario/bin/sag` (`%USERPROFILE%\.sagitario\bin\sag.exe` on Windows) when it exists, where the installers, `nova update self` and `--install` put it; else `sag` on `PATH`. An installation with `--dir` elsewhere sets the setting. |
| Q414 | Messages | When the server cannot start, the warning gives the command that installs Sagitario (`install.sh` or `install.ps1`, as in the README) and the setting, and offers to open it. |
| Q415 | Updates | After `nova update self` the running server is the previous `sag`, renamed to `sag.old`; `Sagitario: Restart the language server` or reopening the editor starts the new one. The extension does not watch the file. |
| Q416 | Tests | The harness checks the version (Q411); the CI job `vscode` packages the extension. |
| Q417 | Documents | This record, the README of the extension, the README, `CONTRIBUTING.md` (the version of a release is in four files), `docs/testing.md` and the roadmap. |

## Alternatives considered

- **A version of its own:** the extension and `sag lsp` change together, and one version for the
  whole toolchain (Q380) tells which extension goes with which `sag`.
- **Only `PATH`:** it fails right after installing, until the editor restarts, and always for
  applications started from the Dock on macOS.
- **`~/.sagitario/bin` before the setting:** the setting is what a user chooses, also to try a
  `sag` built from source.

## Consequences

- The extension of a release is in its GitHub Release, and finds the `sag` of the installers
  without settings.
- Bumping the version of Sagitario changes four files: `compiler/src/version.sag`,
  `compiler/Nova.toml`, `editors/vscode/package.json` and `editors/vscode/package-lock.json`.
