# 0069. Installers

- Status: Accepted
- Date: 2026-10-27
- Completes: [ADR-0068](0068-releases-and-distribution.md) (increment 2 of phase F7, Q379)

## Context

Since increment 1 a tag publishes a GitHub Release with the archive of each platform and
`SHA256SUMS` ([ADR-0068](0068-releases-and-distribution.md)). Installing one still means
extracting it and editing `PATH` by hand. Increment 2 installs a release with one command, as
`rustup` does. ADR-0068 settled the questions it shares with the other increments: downloads go
through the `curl` of the system (Q386), the checksums are checked with the tools of the system
(Q387), a C compiler is required and the installers say how to install one (Q388), and nothing is
signed (Q389).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q390 | Scripts | `install/install.sh`, in POSIX `sh`, for Linux and macOS, and `install/install.ps1`, for Windows PowerShell 5.1 and later. The release workflow publishes both with each release, as assets. |
| Q391 | Use | `curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh \| sh` and `irm https://github.com/Juanestban/sagitario/releases/latest/download/install.ps1 \| iex`. Both URLs need the repository to be public. |
| Q392 | Options | `--version X.Y.Z` (the latest release by default), `--dir DIR` (`~/.sagitario` by default; the tools go to `DIR/bin`), `--no-modify-path`, and `--from URL`, a directory with the archives and `SHA256SUMS`, for tests and mirrors. `install.ps1` takes them as `-Version`, `-Dir`, `-NoModifyPath` and `-From`, and, since `iex` passes no arguments, also reads `SAGITARIO_VERSION`. |
| Q393 | The latest version | The installers download `SHA256SUMS` first, from `releases/latest/download/` or the release `--version` names, and take the archive of their platform from it, and so its version. This needs one request fewer than following the redirection of `releases/latest`, works the same with `--from`, and needs neither the API of GitHub, which limits requests, nor JSON. |
| Q394 | Steps | Find the platform (`uname`; the architecture of the system on Windows), and fail on one that has no release: Windows on AArch64, other architectures, Linux without glibc. Then download `SHA256SUMS` and the archive with `curl --proto '=https,file'` (`curl.exe` on Windows, where `curl` is an alias of `Invoke-WebRequest`), check the archive with `sha256sum`, `shasum -a 256` or `Get-FileHash`, and extract it in a temporary directory that is always removed. Then install `sag` and `nova` as `--install` does (a new file and `mv` on Unix; on Windows, rename the running executable), add `DIR/bin` to `PATH` as `--install` does (`~/.profile`, and `~/.zshrc` on macOS; the `PATH` of the user on Windows, and of the running session, since `iex` runs in it), and report the versions that `sag --version` and `nova --version` print. |
| Q395 | The C compiler | After installing, look for it as `sag` does: `SAG_CC`, else `clang`, else `gcc`, run with `-dumpmachine` (on macOS without the command line tools, `/usr/bin/clang` only offers to install them, so it is not run). If there is none, a warning, not a failure, gives the command that installs one: `apt`, `dnf`, `pacman` or `zypper`, `xcode-select --install`, or `winget install LLVM.LLVM` and the Visual Studio Build Tools with the C++ workload. On Windows the installer also warns when LLVM is installed but not on `PATH`, when the clang found is MinGW, and when the LLVM clang has no Build Tools to link with. |
| Q396 | Warnings of the system (Q389) | Checked on Windows: a file `curl.exe` downloads has no `Zone.Identifier` stream (the Mark of the Web), which is what makes SmartScreen warn. On macOS, where it is not checked yet, `curl` is not an application that sets `com.apple.quarantine`, which is what makes Gatekeeper warn; the release workflow installs and runs the tools there. An archive downloaded with a browser gets one or the other, and is cleared with `Unblock-File` or `xattr -d com.apple.quarantine`. |
| Q397 | Tests | A CI job `installers`, on Windows, Linux and macOS, builds the archive with `--dist`, writes `SHA256SUMS`, installs it with `--from file://… --dir <temporary> --no-modify-path` (on Windows through Windows PowerShell 5.1, as `iex` runs it), and builds and runs a new package with the installed `nova`. After publishing a release, a job of the release workflow installs it on the three systems with the commands of the README and their defaults, and checks the version, `PATH`, `nova new` and `nova run`. |
| Q398 | Uninstalling | Documented: remove `~/.sagitario` and its line in the shell profiles, or its entry in the `PATH` of the user. No `--uninstall`. |
| Q399 | Documents | This record, an "Install" section in the README, and the roadmap and testing.md. |

## Alternatives considered

- **Following the redirection of `releases/latest`** to the tag of the latest release: one more
  request, and nothing to follow with `--from`.
- **The API of GitHub** (`/repos/…/releases/latest`): limited to 60 requests an hour without a
  token, and its answer is JSON, which a shell script cannot parse reliably.
- **`Invoke-WebRequest` on Windows:** it works, but `curl.exe` is what Nova will run (Q386), and is
  much faster in Windows PowerShell 5.1.
- **Failing without a C compiler:** `sag` and `nova` still install correctly, and a C compiler can
  be installed afterwards; the warning says how.
- **Native packages (`.msi`, `.pkg`, `.deb`):** they may follow, as the roadmap says.

## Consequences

- One command installs a release, once the repository is public.
- The scripts run on the systems of the users, not only in CI: they use POSIX `sh` and Windows
  PowerShell 5.1, never `exit` in `install.ps1` (through `iex`, it would close the terminal of the
  user), and are checked out with LF line endings (`.gitattributes`).
- `nova update self` (increment 3) installs the same archives in the same directory.
