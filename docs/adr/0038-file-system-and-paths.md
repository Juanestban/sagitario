# 0038. `std::fs` and `std::path`

- Status: Accepted
- Date: 2026-10-24
- Builds on: [ADR-0037](0037-standard-library-env-io-process.md)

## Context

F3 increment 2 adds the file system to the standard library. File systems differ: Windows paths
are UTF-16 and use `\` and drives, POSIX paths are bytes. Error codes and line-ending
conventions differ too. Increment 2 settles Q91–Q101.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q91 | Paths | `String`, with no `Path` type yet. Names that are not UTF-8 are converted lossily when listed. `/` is a separator everywhere. |
| Q92 | Unicode on Windows | Paths are converted to UTF-16 and passed to the kernel32 `...W` functions, declared by the runtime as in ADR-0037. POSIX systems use `fopen`, `stat`, `opendir` and friends, with `_POSIX_C_SOURCE` since they are not C11. |
| Q93 | Reading and writing | `read_to_string` (not UTF-8: `InvalidData`), `write` (creates or truncates) and `append` (creates if missing). Binary mode: contents are exact, with no `\n`/`\r\n` translation. |
| Q94 | Bytes | `read_bytes` and `write_bytes` wait for an efficient way to pass buffers between the runtime and `List<u8>`. |
| Q95 | Queries | `exists`, `is_file` and `is_dir` return `bool`; an error counts as `false`, as in Rust. Metadata waits for `std::time`. |
| Q96 | Directories | `create_dir`, `create_dir_all` (an existing directory is fine), `remove_dir` (empty only) and `read_dir`, which returns the names without `.` and `..`, sorted by bytes so that every system gives the same order. `remove_dir_all` is deferred: it is recursive and has pitfalls with symbolic links. |
| Q97 | Files | `remove_file`, `rename` (replaces a target file on every system) and `copy`. |
| Q98 | Current directory | `fs::current_dir()`. Changing it waits for F4, like `set_var`, since it is process-wide state. |
| Q99 | Error messages | The message names the operation and the path: `cannot read "notes.txt": No such file or directory`. On Windows, `GetLastError` codes are translated to `errno` values, so kinds and messages match on every system. Using a directory as a file is reported as `EISDIR` everywhere. |
| Q100 | `std::path` | Functions on `String`s: `join`, `file_name`, `parent`, `extension` and `stem`, returning `Option` where there may be no result, with Rust's results for the edge cases. `\` is also a separator on Windows. `join` uses `/`, or `\` when the base uses only `\`. |
| Q101 | Test harness | Each `run-pass` and `run-panic` case runs in a new empty working directory, removed afterwards. |

## Alternatives considered

- **A `Path` type:** stronger typing, but more API surface before packages exist. Plain strings
  can be wrapped later.
- **Rust's messages without the path:** a known complaint. Programs would have to add the path
  to every error themselves.
- **Platform-specific separators in `join`:** mixed separators work on Windows, and keeping `/`
  makes outputs identical across systems unless the program already uses `\`.
- **Unsorted `read_dir`:** the order would differ between systems and runs.

## Consequences

- The runtime declares about fifteen kernel32 functions and one structure (`WIN32_FIND_DATAW`)
  by hand. Paths are limited to `MAX_PATH` (260 characters) on Windows, unless the system
  enables long paths.
- The POSIX code was compiled locally with the Android NDK's clang (bionic). glibc and macOS are
  first exercised in CI.
- `std::fs` builds its `io::Error`s itself, since the error helpers of `std::io` are private and
  the language has no package-private visibility. ADR-0039 replaces this with a helper shared
  through `std`'s own file.
