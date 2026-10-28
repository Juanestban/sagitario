# 0070. `nova update self`

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0068](0068-releases-and-distribution.md) (increment 3 of phase F7, Q379; Q387:
  SHA-256 and JSON)

## Context

Releases (ADR-0068) and installers ([ADR-0069](0069-installers.md)) put `sag` and `nova` in
`~/.sagitario/bin`. Increment 3 keeps them up to date from Nova, as `rustup update` does. ADR-0068
decided that downloads go through the `curl` of the system (Q386), and that SHA-256 and JSON would
live inside the compiler, in `compiler/src/sha256.sag` and a shared `compiler/src/json.sag`
(Q387). Two facts found since make Q387 unnecessary. `std::fs` reads text only: `read_to_string`
rejects a file that is not UTF-8, so Sagitario code cannot read the bytes of an archive to hash
them without a new API of the standard library. And the installers find the latest release in its
`SHA256SUMS` (ADR-0069, Q393), so Nova needs neither the API of GitHub nor JSON.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q400 | The command | `nova update self` installs the latest release; `--check` only says whether there is a newer one; `--version X.Y.Z` installs that release, newer or not, which also goes back to an older one; `--from URL` downloads from a directory with the archives and `SHA256SUMS`, as the installers do, for tests and mirrors. `nova update` without `self` is a usage error: it is kept for dependencies. |
| Q401 | What it updates | The directory of the running Nova: the directory of its path, or the first directory of `PATH` that holds `nova` when it was started by name alone. That directory must hold `sag` too, as a release and `--install` lay them out; otherwise `nova update self` refuses. |
| Q402 | The platform | The triple the installers find: `x86_64-pc-windows-msvc` on Windows; elsewhere from `uname -s` and `uname -m`, AArch64 under Rosetta on macOS. |
| Q403 | The latest version | Nova downloads `SHA256SUMS` from `releases/latest/download/` (or the release `--version` names), takes the archive of its platform and its version from it, and compares that version with its own (`version::VERSION`) part by part. When it is not newer, Nova says that Sagitario is up to date; `--version` installs regardless. |
| Q404 | SHA-256 and JSON (refines Q387) | The SHA-256 of the archive comes from the tools of the system, as for the installers: `sha256sum` or `shasum -a 256`, and `Get-FileHash` through Windows PowerShell. No `compiler/src/sha256.sag` and no JSON: `compiler/src/lsp/json.sag` stays in the language server. Reading bytes (`fs::read_bytes`) and hashing (`std::hash`) wait for records of their own. |
| Q405 | Downloading and extracting | With the `curl` and the `tar` of the system (on Windows, those of `System32`: PATH may hold the `tar` of Git, which reads no `.zip`), in a directory `.update` inside the directory of the installation, removed at the end, also after a failure. Extracted there, the new executables keep the mode `tar` gives them (`fs::copy` keeps none on Unix) and are renamed into place within one file system. |
| Q406 | Replacing | `sag`, then `nova`: each is renamed to `<tool>.old` (`.old.exe` on Windows), replacing the previous one, and the new one is renamed to its place. A running executable can be renamed, also on Windows. If a step fails, the tools already replaced are put back. The previous version stays as `.old` until the next update. |
| Q407 | Reporting | `Updating Sagitario from A to B`, then what the new `sag --version` and `nova --version` print, and the directory. |
| Q408 | Tests | `nova::updates_itself_from_a_release` in the harness builds a release `99.0.0` with the `sag` and the Nova under test, `tar` and `SHA256SUMS`, installs them in a temporary directory, and checks `--check`, the update (the tools replaced, the previous ones kept, `.update` removed), an installation that is up to date, a wrong checksum that changes nothing, a missing version and the usage errors. |
| Q409 | Documents | This record, `docs/nova.md`, the usage of `nova`, the roadmap and the README. |

## Alternatives considered

- **SHA-256 in Sagitario (`compiler/src/sha256.sag`, Q387):** it needs the bytes of the archive,
  which the standard library cannot read; adding `fs::read_bytes` is library design for its own
  record.
- **The API of GitHub and JSON:** limited to 60 requests an hour without a token, and unnecessary
  once `SHA256SUMS` names the archives.
- **`--rollback`:** `--version` with the previous version does the same, and works for any
  release.
- **Replacing in place with `fs::copy`:** it loses the executable mode on Unix, and a running
  executable cannot be overwritten on Windows.

## Consequences

- `nova update self` works where the installers do: `curl`, `tar` and a hashing tool are on every
  supported system (Windows 10 1803 and later ship `curl.exe` and `tar.exe`).
- An update leaves `sag.old` and `nova.old` beside the tools, as `install.ps1` and `build.ps1
  --install` already do on Windows.
- One command remains of F7: publishing the VS Code extension (increment 4).
