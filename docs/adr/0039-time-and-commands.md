# 0039. `std::time` and `std::process::Command`

- Status: Accepted
- Date: 2026-10-24
- Builds on: [ADR-0037](0037-standard-library-env-io-process.md),
  [ADR-0038](0038-file-system-and-paths.md)

## Context

F3 increment 3 adds clocks and running other programs. The language has no operator
overloading and no threads yet. Running a child and capturing both of its output streams
deadlocks if done naively: the child blocks on a full pipe while the parent waits on the other.
Increment 3 settles Q102–Q113.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q102 | `Duration` | A struct holding nanoseconds in a private `u64` (about 584 years). `from_secs/millis/micros/nanos`, `as_secs/millis/micros/nanos`, `as_secs_f64`, and arithmetic as methods: `plus`, `minus` (panics when it would be negative, as in Rust) and `times`. It implements `Ord`. |
| Q103 | `Instant` | Monotonic: `now()`, `elapsed()` and `duration_since(earlier)`, which saturates at zero. `clock_gettime(CLOCK_MONOTONIC)` on POSIX, `QueryPerformanceCounter` on Windows. |
| Q104 | `SystemTime` | `now()` and `since_unix_epoch()`, zero before 1970. Calendars, time zones and formatting are out of scope. |
| Q105 | `sleep` | `time::sleep(d)`, in `std::time` until `std::thread` exists (F4). |
| Q106 | API | `Command` is a class whose builder methods change it and return it: `new Command("git").arg("status")`. |
| Q107 | Running | `output()` captures stdout and stderr; `status()` inherits them. `Output { status, stdout, stderr }` and `ExitStatus { code: Option<i32> }` with `success()`. |
| Q108 | Child's stdin | `input(text)` writes the text and closes stdin; without it the child inherits stdin, as in Rust. |
| Q109 | Output that is not UTF-8 | Converted lossily (U+FFFD), not an error. Bytes wait for an efficient `List<u8>` (Q94). |
| Q110 | Lookup and errors | The program is looked up in `PATH`. A missing program is a `NotFound` error naming it. A non-zero exit code is not an error. |
| Q111 | Implementation | No threads visible to programs. POSIX uses `fork` and `execve` after an own `PATH` lookup, a close-on-exec pipe to report a failed `execve`, and `poll` to read both streams while writing stdin. Windows uses `CreateProcessW` with one runtime thread per captured pipe, and quotes arguments with the inverse of the rules of ADR-0037. |
| Q112 | Deferred | `spawn` (a child in the background with `wait` and `kill`), redirecting to files, `env_remove`/`env_clear`, and pipes between processes. |
| Q113 | Tests | `Command` tests run the test program itself as the child, with a role in its first argument. Clock tests use tolerant bounds. |

## Alternatives considered

- **A single integer of nanoseconds instead of `Duration`:** no type safety between seconds and
  milliseconds.
- **Temporary files instead of pipes for captured output:** no deadlocks without threads or
  `poll`, but slower, and files can be left behind.
- **`posix_spawn` instead of `fork`:** changing the child's directory needs
  `posix_spawn_file_actions_addchdir_np`, which is not available on every supported system.
- **Testing with `echo` or `cmd`:** different on every system.

## Consequences

- The runtime declares the Windows process, pipe and thread functions and their structures
  itself, like those of ADR-0037 and ADR-0038.
- On Windows, a Sagitario program writing to a pipe or a file ends lines with `\r\n`, since the
  C runtime's stdout is in text mode. The `Command` tests accept both endings. Whether stdout
  should be binary on Windows is left open.
- Several standard library modules build `io::Error`s from runtime codes. `std` gets a file of
  its own whose private helpers only its modules see, through the existing rule that submodules
  see their ancestors' private items. `std::io` and `std::fs` now share `super::os_error` too.
